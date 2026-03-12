use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::AgentError;

/// A conversation message from the database.
pub struct ConversationMessage {
    pub role: String,
    pub content: Value,
}

/// Load the last `limit` messages for a user from the `conversations` table.
///
/// Returns messages in chronological order (oldest first).
pub async fn load_history(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ConversationMessage>, AgentError> {
    let rows: Vec<(String, Value)> = sqlx::query_as(
        r#"
        SELECT role, content
        FROM conversations
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await?;

    // Reverse to get chronological order
    Ok(rows
        .into_iter()
        .rev()
        .map(|(role, content)| ConversationMessage { role, content })
        .collect())
}

/// Save a message (user or assistant) to the `conversations` table.
///
/// Returns the UUID of the newly created row.
pub async fn save_message(
    db: &PgPool,
    user_id: Uuid,
    role: &str,
    content: &Value,
) -> Result<Uuid, AgentError> {
    let (id,): (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO conversations (user_id, role, content)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(role)
    .bind(content)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// Log an agent event to the `agent_events` table for auditing and analytics.
///
/// Records the trigger type, which tools were used, and the response latency.
pub async fn log_agent_event(
    db: &PgPool,
    user_id: Uuid,
    trigger_type: &str,
    tools_used: Option<Value>,
    latency_ms: i32,
) -> Result<Uuid, AgentError> {
    let (id,): (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO agent_events (user_id, trigger_type, tools_used, latency_ms)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(trigger_type)
    .bind(tools_used)
    .bind(latency_ms)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// Prune old messages, keeping only the most recent `keep` messages per user.
///
/// Prevents unbounded growth of the conversations table.
pub async fn prune_old_messages(
    db: &PgPool,
    user_id: Uuid,
    keep: i64,
) -> Result<(), AgentError> {
    sqlx::query(
        r#"
        DELETE FROM conversations
        WHERE user_id = $1
          AND id NOT IN (
              SELECT id FROM conversations
              WHERE user_id = $1
              ORDER BY created_at DESC
              LIMIT $2
          )
        "#,
    )
    .bind(user_id)
    .bind(keep)
    .execute(db)
    .await?;

    Ok(())
}
