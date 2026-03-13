use sqlx::PgPool;
use uuid::Uuid;

use crate::agent::AgentError;

/// A conversation message from the `conversations` table.
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
}

/// Load conversation history (newest last = chronological).
pub async fn load_history(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ConversationMessage>, AgentError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"SELECT role, content FROM conversations
           WHERE user_id = $1
           ORDER BY created_at DESC
           LIMIT $2"#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await?;

    // Reverse to get chronological order
    let messages: Vec<ConversationMessage> = rows
        .into_iter()
        .rev()
        .map(|(role, content)| ConversationMessage { role, content })
        .collect();

    Ok(messages)
}

/// Save a message to conversation history.
/// The `content` parameter is a serde_json::Value for backward compat with the agent,
/// but we store it as TEXT.
pub async fn save_message(
    db: &PgPool,
    user_id: Uuid,
    role: &str,
    content: &serde_json::Value,
) -> Result<(), AgentError> {
    let text = match content {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    sqlx::query(
        "INSERT INTO conversations (user_id, role, content) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(role)
    .bind(&text)
    .execute(db)
    .await?;

    Ok(())
}

/// Prune old messages, keeping only the most recent `keep` messages.
pub async fn prune_old_messages(
    db: &PgPool,
    user_id: Uuid,
    keep: i64,
) -> Result<(), AgentError> {
    sqlx::query(
        r#"DELETE FROM conversations
           WHERE user_id = $1 AND id NOT IN (
               SELECT id FROM conversations
               WHERE user_id = $1
               ORDER BY created_at DESC
               LIMIT $2
           )"#,
    )
    .bind(user_id)
    .bind(keep)
    .execute(db)
    .await?;

    Ok(())
}

/// Log an agent event for audit trail.
pub async fn log_agent_event(
    db: &PgPool,
    user_id: Uuid,
    trigger_type: &str,
    tools_used: Option<serde_json::Value>,
    latency_ms: i32,
) -> Result<(), AgentError> {
    sqlx::query(
        r#"INSERT INTO agent_events (user_id, trigger_type, tools_used, latency_ms)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(user_id)
    .bind(trigger_type)
    .bind(tools_used)
    .bind(latency_ms)
    .execute(db)
    .await?;

    Ok(())
}
