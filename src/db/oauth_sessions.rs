use sqlx::PgPool;

pub struct OAuthSession {
    pub jwt: String,
    pub email: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
    pub is_new: bool,
}

pub async fn create(
    db: &PgPool,
    jwt: &str,
    email: &str,
    display_name: Option<&str>,
    is_admin: bool,
    is_new: bool,
) -> Result<String, sqlx::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO oauth_sessions (jwt, email, display_name, is_admin, is_new)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id"#,
        jwt,
        email,
        display_name,
        is_admin,
        is_new,
    )
    .fetch_one(db)
    .await?;

    Ok(row.id.to_string())
}

/// Consumes (deletes) the session and returns its data.
/// Sessions expire after 10 minutes.
pub async fn consume(
    db: &PgPool,
    session_id: &str,
) -> Result<Option<OAuthSession>, sqlx::Error> {
    let Ok(uuid) = uuid::Uuid::parse_str(session_id) else {
        return Ok(None);
    };

    let row = sqlx::query!(
        r#"DELETE FROM oauth_sessions
           WHERE id = $1 AND created_at > NOW() - INTERVAL '10 minutes'
           RETURNING jwt, email, display_name, is_admin, is_new"#,
        uuid,
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| OAuthSession {
        jwt: r.jwt,
        email: r.email,
        display_name: r.display_name,
        is_admin: r.is_admin,
        is_new: r.is_new,
    }))
}
