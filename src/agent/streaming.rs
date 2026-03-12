use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;

use crate::{auth::Claims, AppState};

use super::{AgentTrigger, CoachAgent, StreamEvent};

/// WebSocket input message from the client.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsInput {
    Message { content: String },
}

/// GET /api/ws — WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    claims: Claims,
) -> impl IntoResponse {
    let agent = CoachAgent::new(
        state.db.clone(),
        state.config.clone(),
        state.http.clone(),
    );
    ws.on_upgrade(move |socket| handle_socket(socket, agent, claims.sub))
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
                    WsInput::Message { content } => {
                        let trigger = AgentTrigger::ChatMessage { content };

                        // Create channel for streaming events
                        let (tx, mut rx) =
                            tokio::sync::mpsc::channel::<StreamEvent>(64);

                        // Spawn agent processing in background
                        let agent_clone = agent.clone();
                        let uid = user_id;
                        tokio::spawn(async move {
                            if let Err(e) = agent_clone.handle(uid, trigger, Some(tx)).await {
                                tracing::error!("Agent error for user {}: {}", uid, e);
                            }
                        });

                        // Stream events to the WebSocket
                        while let Some(event) = rx.recv().await {
                            let json = serde_json::to_string(&event).unwrap_or_default();
                            if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
