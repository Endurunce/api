use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub struct StravaTokenRow {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub athlete_id: i64,
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
