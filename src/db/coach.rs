use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoachMessage {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

pub async fn save_message(
    db: &PgPool,
    user_id: Uuid,
    role: &str,
    content: &str,
) -> Result<CoachMessage, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        INSERT INTO coach_messages (user_id, role, content)
        VALUES ($1, $2, $3)
        RETURNING id, role, content, created_at
        "#,
        user_id,
        role,
        content,
    )
    .fetch_one(db)
    .await?;

    Ok(CoachMessage {
        id: row.id,
        role: row.role,
        content: row.content,
        created_at: row.created_at,
    })
}

pub async fn fetch_messages(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<CoachMessage>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT id, role, content, created_at
        FROM coach_messages
        WHERE user_id = $1
        ORDER BY created_at ASC
        LIMIT $2
        "#,
        user_id,
        limit,
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| CoachMessage {
            id: r.id,
            role: r.role,
            content: r.content,
            created_at: r.created_at,
        })
        .collect())
}

/// Keep the newest `keep` messages per user, delete the rest.
pub async fn prune_old_messages(
    db: &PgPool,
    user_id: Uuid,
    keep: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        DELETE FROM coach_messages
        WHERE user_id = $1
          AND id NOT IN (
              SELECT id FROM coach_messages
              WHERE user_id = $1
              ORDER BY created_at DESC
              LIMIT $2
          )
        "#,
        user_id,
        keep,
    )
    .execute(db)
    .await?;
    Ok(())
}
