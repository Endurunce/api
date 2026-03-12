use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub struct StravaTokenRow {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub athlete_id: i64,
}

pub struct AthleteInfo {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

pub async fn upsert_tokens(
    db: &PgPool,
    user_id: Uuid,
    athlete_id: i64,
    access_token: &str,
    refresh_token: &str,
    expires_at: DateTime<Utc>,
    scope: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO strava_tokens (user_id, athlete_id, access_token, refresh_token, expires_at, scope)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (user_id) DO UPDATE SET
            athlete_id    = EXCLUDED.athlete_id,
            access_token  = EXCLUDED.access_token,
            refresh_token = EXCLUDED.refresh_token,
            expires_at    = EXCLUDED.expires_at,
            scope         = EXCLUDED.scope,
            updated_at    = NOW()
        "#,
        user_id,
        athlete_id,
        access_token,
        refresh_token,
        expires_at,
        scope,
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Upsert tokens including user-provided Strava API credentials
pub async fn upsert_tokens_with_credentials(
    db: &PgPool,
    user_id: Uuid,
    athlete_id: i64,
    access_token: &str,
    refresh_token: &str,
    expires_at: DateTime<Utc>,
    scope: &str,
    strava_client_id: &str,
    strava_client_secret: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO strava_tokens (user_id, athlete_id, access_token, refresh_token, expires_at, scope, strava_client_id, strava_client_secret, display_name, avatar_url)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (user_id) DO UPDATE SET
            athlete_id          = EXCLUDED.athlete_id,
            access_token        = EXCLUDED.access_token,
            refresh_token       = EXCLUDED.refresh_token,
            expires_at          = EXCLUDED.expires_at,
            scope               = EXCLUDED.scope,
            strava_client_id    = EXCLUDED.strava_client_id,
            strava_client_secret= EXCLUDED.strava_client_secret,
            display_name        = EXCLUDED.display_name,
            avatar_url          = EXCLUDED.avatar_url,
            updated_at          = NOW()
        "#,
    )
    .bind(user_id)
    .bind(athlete_id)
    .bind(access_token)
    .bind(refresh_token)
    .bind(expires_at)
    .bind(scope)
    .bind(strava_client_id)
    .bind(strava_client_secret)
    .bind(display_name)
    .bind(avatar_url)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn fetch_tokens(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Option<StravaTokenRow>, sqlx::Error> {
    let row = sqlx::query_as!(
        StravaTokenRow,
        r#"
        SELECT access_token, refresh_token, expires_at, athlete_id
        FROM strava_tokens
        WHERE user_id = $1
        "#,
        user_id,
    )
    .fetch_optional(db)
    .await?;

    Ok(row)
}

/// Fetch athlete display info (display_name, avatar_url) from strava_tokens
pub async fn fetch_athlete_info(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Option<AthleteInfo>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT display_name, avatar_url FROM strava_tokens WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|(display_name, avatar_url)| AthleteInfo {
        display_name,
        avatar_url,
    }))
}
