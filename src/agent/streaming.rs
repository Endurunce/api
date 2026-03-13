use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;

use crate::{auth, errors::AppError, AppState};

use super::{intake, AgentTrigger, CoachAgent, StreamEvent};

/// WebSocket input message from the client.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsInput {
    Message { content: String },
    StartIntake,
    QuickReply { value: String },
}

/// Query parameters for WebSocket auth (browsers can't send headers on WS).
#[derive(Debug, Deserialize)]
pub struct WsAuth {
    token: String,
}

/// GET /api/ws?token=JWT — WebSocket upgrade handler for the AI coach agent.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(auth_query): Query<WsAuth>,
) -> Result<impl IntoResponse, AppError> {
    let claims = auth::decode_token(&auth_query.token, &state.config.jwt_secret)?;
    let agent = CoachAgent::new(
        state.db.clone(),
        state.config.clone(),
        state.http.clone(),
    );
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, agent, claims.sub)))
}

async fn handle_socket(socket: WebSocket, agent: CoachAgent, user_id: uuid::Uuid) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                let input: WsInput = match serde_json::from_str(&text) {
                    Ok(i) => i,
                    Err(e) => {
                        let err_event = StreamEvent::Error {
                            message: format!("Invalid message format: {}", e),
                        };
                        let _ = ws_tx
                            .send(Message::Text(
                                serde_json::to_string(&err_event).unwrap_or_default().into(),
                            ))
                            .await;
                        continue;
                    }
                };

                match input {
                    WsInput::StartIntake => {
                        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

                        let uid = user_id;
                        tokio::spawn(async move {
                            if let Err(e) = intake::start_intake(uid, &tx).await {
                                tracing::error!("Intake start error for user {}: {}", uid, e);
                            }
                        });

                        while let Some(event) = rx.recv().await {
                            let json = serde_json::to_string(&event).unwrap_or_default();
                            if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    WsInput::QuickReply { value } | WsInput::Message { content: value }
                        if intake::has_active_intake(user_id).await =>
                    {
                        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

                        let agent_clone = agent.clone();
                        let uid = user_id;
                        let val = value.clone();
                        tokio::spawn(async move {
                            if let Err(e) = intake::handle_reply(uid, &val, &tx, &agent_clone).await {
                                tracing::error!("Intake reply error for user {}: {}", uid, e);
                            }
                        });

                        while let Some(event) = rx.recv().await {
                            let json = serde_json::to_string(&event).unwrap_or_default();
                            if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    WsInput::Message { content } => {
                        let trigger = AgentTrigger::ChatMessage { content };

                        let (tx, mut rx) =
                            tokio::sync::mpsc::channel::<StreamEvent>(64);

                        let agent_clone = agent.clone();
                        let uid = user_id;
                        tokio::spawn(async move {
                            if let Err(e) = agent_clone.handle(uid, trigger, Some(tx)).await {
                                tracing::error!("Agent error for user {}: {}", uid, e);
                            }
                        });

                        while let Some(event) = rx.recv().await {
                            let json = serde_json::to_string(&event).unwrap_or_default();
                            if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    WsInput::QuickReply { .. } => {
                        // Quick reply outside of intake — ignore
                        let err_event = StreamEvent::Error {
                            message: "Quick reply niet verwacht buiten intake.".into(),
                        };
                        let _ = ws_tx
                            .send(Message::Text(
                                serde_json::to_string(&err_event).unwrap_or_default().into(),
                            ))
                            .await;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
