use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{
    agent::{self, intake as intake_handler, CoachAgent, StreamEvent},
    auth::Claims,
    errors::{AppError, ApiResult},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct IntakeStartResponse {
    pub question: String,
    pub question_id: Option<String>,
    pub quick_replies: Option<Vec<agent::QuickReply>>,
}

#[derive(Debug, Deserialize)]
pub struct IntakeReplyRequest {
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct IntakeReplyResponse {
    pub events: Vec<serde_json::Value>,
    pub intake_active: bool,
}

/// POST /api/intake/start — Start the intake flow for a new user.
pub async fn start(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<(StatusCode, Json<IntakeStartResponse>)> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

    intake_handler::start_intake(claims.sub, &tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

    drop(tx); // Close sender so receiver finishes

    let mut question = String::new();
    let mut question_id = None;
    let mut quick_replies = None;

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::TextDelta { delta } => question.push_str(&delta),
            StreamEvent::QuickReplies { question_id: qid, options } => {
                question_id = Some(qid);
                quick_replies = Some(options);
            }
            _ => {}
        }
    }

    Ok((StatusCode::OK, Json(IntakeStartResponse {
        question,
        question_id,
        quick_replies,
    })))
}

/// POST /api/intake/reply — Send a reply to the current intake step.
pub async fn reply(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<IntakeReplyRequest>,
) -> ApiResult<Json<IntakeReplyResponse>> {
    let agent = CoachAgent::new(
        state.db.clone(),
        state.config.clone(),
        state.http.clone(),
    );

    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

    let uid = claims.sub;
    let val = req.value.clone();
    let agent_clone = agent.clone();

    let intake_result = tokio::spawn(async move {
        intake_handler::handle_reply(uid, &val, &tx, &agent_clone).await
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Task join error: {}", e)))?
    .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(serde_json::to_value(&event).unwrap_or_default());
    }

    Ok(Json(IntakeReplyResponse {
        events,
        intake_active: intake_result,
    }))
}
