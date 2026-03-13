use axum::{extract::State, Json};
use serde::Serialize;

use crate::{agent::memory, auth::Claims, errors::AppError, AppState};

#[derive(Serialize)]
pub struct ConversationMessageResponse {
    pub role: String,
    pub content: String,
}

/// GET /api/conversations — Returns the user's conversation history.
///
/// **Auth:** Bearer JWT required.
///
/// Returns the last 40 messages in chronological order (oldest first).
pub async fn list(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<Vec<ConversationMessageResponse>>, AppError> {
    let messages = memory::load_history(&state.db, claims.sub, 40)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

    let response: Vec<ConversationMessageResponse> = messages
        .into_iter()
        .map(|m| ConversationMessageResponse {
            role: m.role,
            content: m.content,
        })
        .collect();

    Ok(Json(response))
}
