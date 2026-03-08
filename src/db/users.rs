use sqlx::PgPool;
use uuid::Uuid;
use serde::Serialize;

pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
}

#[derive(Debug, Serialize)]
pub struct AdminUserRow {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub strava_id: Option<i64>,
    pub google_id: Option<String>,
    pub is_admin: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn insert(db: &PgPool, email: &str, password_hash: &str) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        INSERT INTO users (email, password_hash)
        VALUES ($1, $2)
        RETURNING id
        "#,
        email,
        password_hash,
    )
    .fetch_one(db)
    .await?;

    Ok(row.id)
}

pub async fn fetch_by_email(db: &PgPool, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT id, email, password_hash FROM users WHERE email = $1",
        email,
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| UserRow {
        id: r.id,
        email: r.email,
        password_hash: r.password_hash,
    }))
}

pub async fn exists(db: &PgPool, email: &str) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!("SELECT 1 AS one FROM users WHERE email = $1", email)
        .fetch_optional(db)
        .await?;
    Ok(row.is_some())
}

/// Fetch the is_admin flag for a user (used after login to include in JWT)
pub async fn fetch_is_admin(db: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let row = sqlx::query_as::<_, (bool,)>("SELECT is_admin FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(row.map(|(v,)| v).unwrap_or(false))
}

/// Find user by strava_id, or create one. Returns (id, email, is_admin).
pub async fn find_or_create_by_strava(
    db: &PgPool,
    strava_id: i64,
    email: Option<&str>,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
) -> Result<(Uuid, String, bool), sqlx::Error> {
    // 1. Find by strava_id
    let existing = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, email, is_admin FROM users WHERE strava_id = $1"
    )
    .bind(strava_id)
    .fetch_optional(db)
    .await?;

    if let Some((id, em, is_admin)) = existing {
        // Update display/avatar in case they changed
        sqlx::query(
            "UPDATE users SET display_name = COALESCE($1, display_name), avatar_url = COALESCE($2, avatar_url) WHERE id = $3"
        )
        .bind(display_name)
        .bind(avatar_url)
        .bind(id)
        .execute(db)
        .await?;
        return Ok((id, em, is_admin));
    }

    // 2. Find by email (link existing account)
    if let Some(email) = email {
        let by_email = sqlx::query_as::<_, (Uuid, String, bool)>(
            "SELECT id, email, is_admin FROM users WHERE email = $1"
        )
        .bind(email)
        .fetch_optional(db)
        .await?;

        if let Some((id, em, is_admin)) = by_email {
            sqlx::query(
                "UPDATE users SET strava_id = $1, display_name = COALESCE($2, display_name), avatar_url = COALESCE($3, avatar_url) WHERE id = $4"
            )
            .bind(strava_id)
            .bind(display_name)
            .bind(avatar_url)
            .bind(id)
            .execute(db)
            .await?;
            return Ok((id, em, is_admin));
        }
    }

    // 3. Create new user
    let placeholder_email = email
        .map(|e| e.to_string())
        .unwrap_or_else(|| format!("strava_{}@endurunce.nl", strava_id));
    let placeholder_hash = format!("oauth_{}", uuid::Uuid::new_v4());

    let row = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO users (email, password_hash, strava_id, display_name, avatar_url) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind(&placeholder_email)
    .bind(&placeholder_hash)
    .bind(strava_id)
    .bind(display_name)
    .bind(avatar_url)
    .fetch_one(db)
    .await?;

    Ok((row.0, placeholder_email, false))
}

/// Find user by google_id, or create one. Returns (id, email, is_admin).
pub async fn find_or_create_by_google(
    db: &PgPool,
    google_id: &str,
    email: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
) -> Result<(Uuid, String, bool), sqlx::Error> {
    // 1. Find by google_id
    let existing = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, email, is_admin FROM users WHERE google_id = $1"
    )
    .bind(google_id)
    .fetch_optional(db)
    .await?;

    if let Some((id, em, is_admin)) = existing {
        sqlx::query(
            "UPDATE users SET display_name = COALESCE($1, display_name), avatar_url = COALESCE($2, avatar_url) WHERE id = $3"
        )
        .bind(display_name)
        .bind(avatar_url)
        .bind(id)
        .execute(db)
        .await?;
        return Ok((id, em, is_admin));
    }

    // 2. Find by email (link existing account)
    let by_email = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, email, is_admin FROM users WHERE email = $1"
    )
    .bind(email)
    .fetch_optional(db)
    .await?;

    if let Some((id, em, is_admin)) = by_email {
        sqlx::query(
            "UPDATE users SET google_id = $1, display_name = COALESCE($2, display_name), avatar_url = COALESCE($3, avatar_url) WHERE id = $4"
        )
        .bind(google_id)
        .bind(display_name)
        .bind(avatar_url)
        .bind(id)
        .execute(db)
        .await?;
        return Ok((id, em, is_admin));
    }

    // 3. Create new user
    let placeholder_hash = format!("oauth_{}", uuid::Uuid::new_v4());

    let row = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO users (email, password_hash, google_id, display_name, avatar_url) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind(email)
    .bind(&placeholder_hash)
    .bind(google_id)
    .bind(display_name)
    .bind(avatar_url)
    .fetch_one(db)
    .await?;

    Ok((row.0, email.to_string(), false))
}

/// List all users for admin panel
pub async fn fetch_all_admin(db: &PgPool, page: i64, per_page: i64, search: Option<&str>) -> Result<(Vec<AdminUserRow>, i64), sqlx::Error> {
    let offset = (page - 1) * per_page;
    let pattern = search.map(|s| format!("%{}%", s));

    let users = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<String>, Option<i64>, Option<String>, bool, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, email, display_name, avatar_url, strava_id, google_id, is_admin, created_at
           FROM users
           WHERE ($1::text IS NULL OR email ILIKE $1 OR display_name ILIKE $1)
           ORDER BY created_at DESC
           LIMIT $2 OFFSET $3"#
    )
    .bind(pattern.as_deref())
    .bind(per_page)
    .bind(offset)
    .fetch_all(db)
    .await?;

    let total = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM users WHERE ($1::text IS NULL OR email ILIKE $1 OR display_name ILIKE $1)"
    )
    .bind(pattern.as_deref())
    .fetch_one(db)
    .await
    .map(|(c,)| c)?;

    let rows = users.into_iter().map(|(id, email, display_name, avatar_url, strava_id, google_id, is_admin, created_at)| {
        AdminUserRow { id, email, display_name, avatar_url, strava_id, google_id, is_admin, created_at }
    }).collect();

    Ok((rows, total))
}

pub async fn set_admin(db: &PgPool, user_id: Uuid, is_admin: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET is_admin = $1 WHERE id = $2")
        .bind(is_admin)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Aggregated stats for admin dashboard
pub async fn fetch_stats(db: &PgPool) -> Result<serde_json::Value, sqlx::Error> {
    let (total_users,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM users")
        .fetch_one(db).await?;
    let (strava_connected,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM users WHERE strava_id IS NOT NULL")
        .fetch_one(db).await?;
    let (google_connected,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM users WHERE google_id IS NOT NULL")
        .fetch_one(db).await?;
    let (total_plans,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM plans")
        .fetch_one(db).await?;
    let (active_plans,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM plans WHERE active = true")
        .fetch_one(db).await?;
    let (total_injuries,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM injury_reports")
        .fetch_one(db).await?;
    let (active_injuries,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM injury_reports WHERE recovery_status = 'active'")
        .fetch_one(db).await?;
    let (new_users_7d,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM users WHERE created_at > NOW() - INTERVAL '7 days'")
        .fetch_one(db).await?;

    Ok(serde_json::json!({
        "total_users": total_users,
        "strava_connected": strava_connected,
        "google_connected": google_connected,
        "total_plans": total_plans,
        "active_plans": active_plans,
        "total_injuries": total_injuries,
        "active_injuries": active_injuries,
        "new_users_7d": new_users_7d,
    }))
}
