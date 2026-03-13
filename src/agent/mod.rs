pub mod context;
pub mod intake;
pub mod memory;
pub mod streaming;
pub mod tools;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;

/// All possible triggers that can invoke the coach agent.
///
/// Each variant represents a different entry point (chat, injury report,
/// session feedback, scheduled check-in, or week rollover).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentTrigger {
    /// User sent a chat message
    ChatMessage { content: String },
    /// User reported an injury via form
    InjuryReport {
        locations: Vec<String>,
        severity: u8,
        can_walk: bool,
        can_run: bool,
        description: Option<String>,
    },
    /// Session feedback submitted
    SessionFeedback {
        plan_id: Uuid,
        week: u32,
        day: u8,
        feeling: u8,
        notes: Option<String>,
    },
    /// Start the conversational intake flow (new user onboarding)
    StartIntake,
    /// Daily morning check-in (cron-triggered)
    DailyCheckIn,
    /// Week rollover
    WeekRollover { new_week: u32 },
}

/// The AI coach agent — orchestrates tool-augmented conversations with Claude.
///
/// Holds shared references to the database pool, HTTP client, and application config.
/// Cloneable and safe to share across tasks.
#[derive(Clone)]
pub struct CoachAgent {
    pub db: PgPool,
    pub config: Config,
    pub http: reqwest::Client,
}

impl CoachAgent {
    /// Create a new coach agent with the given dependencies.
    pub fn new(db: PgPool, config: Config, http: reqwest::Client) -> Self {
        Self { db, config, http }
    }

    /// Simple one-shot chat: send a prompt, get a text response. No tools, no history.
    /// Used for structured generation tasks like plan creation.
    pub async fn chat_single(&self, prompt: &str) -> Result<String, AgentError> {
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": prompt
        })];

        let body = serde_json::json!({
            "model": &self.config.anthropic_model,
            "max_tokens": 16000,
            "messages": messages,
        });

        let api_key = self.config.anthropic_api_key.as_deref()
            .ok_or_else(|| AgentError::Config("ANTHROPIC_API_KEY not set".into()))?;

        let resp = self.http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::Api(e.to_string()))?;

        let data: serde_json::Value = resp.json().await
            .map_err(|e| AgentError::Api(e.to_string()))?;

        let text = data["content"]
            .as_array()
            .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
            .and_then(|b| b["text"].as_str())
            .unwrap_or("")
            .to_string();

        Ok(text)
    }

    /// Main entry point: process any trigger for a user.
    /// Returns the final assistant message text.
    /// If `delta_tx` is provided, streams text deltas and tool events.
    pub async fn handle(
        &self,
        user_id: Uuid,
        trigger: AgentTrigger,
        delta_tx: Option<tokio::sync::mpsc::Sender<StreamEvent>>,
    ) -> Result<String, AgentError> {
        let start = std::time::Instant::now();

        // 1. Build context (system prompt with user data)
        let system_prompt = context::build_system_prompt(&self.db, user_id).await?;

        // 2. Convert trigger to user message
        let user_message = trigger_to_message(&trigger);

        // 3. Load conversation history
        let history = memory::load_history(&self.db, user_id, 20).await?;

        // 4. Build messages array
        let mut messages: Vec<serde_json::Value> = history
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            })
            .collect();
        messages.push(serde_json::json!({
            "role": "user",
            "content": user_message
        }));

        // 5. Save user message to conversation history
        memory::save_message(&self.db, user_id, "user", &serde_json::json!(user_message))
            .await?;

        // 6. Call Claude with tool use in a loop (handle tool calls)
        let tool_defs = tools::tool_definitions();
        let mut full_response = String::new();
        let mut tools_used: Vec<serde_json::Value> = Vec::new();

        loop {
            let response = self
                .call_anthropic(&system_prompt, &messages, &tool_defs)
                .await?;

            let stop_reason = response["stop_reason"]
                .as_str()
                .unwrap_or("end_turn")
                .to_string();

            // Process content blocks
            let content_blocks = response["content"]
                .as_array()
                .cloned()
                .unwrap_or_default();

            let mut assistant_content = Vec::new();

            for block in &content_blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        let text = block["text"].as_str().unwrap_or("");
                        full_response.push_str(text);
                        assistant_content.push(block.clone());

                        if let Some(tx) = &delta_tx {
                            let _ = tx
                                .send(StreamEvent::TextDelta {
                                    delta: text.to_string(),
                                })
                                .await;
                        }
                    }
                    Some("tool_use") => {
                        let tool_name =
                            block["name"].as_str().unwrap_or("unknown").to_string();
                        let tool_id = block["id"].as_str().unwrap_or("").to_string();
                        let tool_input = block["input"].clone();

                        assistant_content.push(block.clone());
                        tools_used.push(serde_json::json!({
                            "name": tool_name,
                            "input": tool_input,
                        }));

                        if let Some(tx) = &delta_tx {
                            let _ = tx
                                .send(StreamEvent::ToolUse {
                                    tool: tool_name.clone(),
                                    id: tool_id.clone(),
                                    input: tool_input.clone(),
                                })
                                .await;
                        }

                        // Execute the tool
                        let result = tools::execute_tool(
                            &self.db,
                            user_id,
                            &tool_name,
                            &tool_input,
                        )
                        .await;

                        let tool_result_content = match &result {
                            Ok(val) => serde_json::to_string(val).unwrap_or_default(),
                            Err(e) => format!("Error: {}", e),
                        };

                        if let Some(tx) = &delta_tx {
                            let _ = tx
                                .send(StreamEvent::ToolResult {
                                    id: tool_id.clone(),
                                    result: tool_result_content.clone(),
                                })
                                .await;
                        }

                        // We'll add the tool result after processing all blocks
                        // But we need to add the assistant message first, then tool result
                        messages.push(serde_json::json!({
                            "role": "assistant",
                            "content": assistant_content.clone()
                        }));
                        messages.push(serde_json::json!({
                            "role": "user",
                            "content": [{
                                "type": "tool_result",
                                "tool_use_id": tool_id,
                                "content": tool_result_content,
                            }]
                        }));
                        assistant_content.clear();
                    }
                    _ => {}
                }
            }

            // If the last block was text (not tool_use), add assistant message
            if !assistant_content.is_empty() {
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": assistant_content
                }));
            }

            // If stop_reason is not tool_use, we're done
            if stop_reason != "tool_use" {
                break;
            }
        }

        // 7. Save assistant response
        memory::save_message(
            &self.db,
            user_id,
            "assistant",
            &serde_json::json!(full_response),
        )
        .await?;

        // 8. Log agent event
        let latency_ms = start.elapsed().as_millis() as i32;
        memory::log_agent_event(
            &self.db,
            user_id,
            &trigger_type_name(&trigger),
            if tools_used.is_empty() {
                None
            } else {
                Some(serde_json::json!(tools_used))
            },
            latency_ms,
        )
        .await?;

        // 9. Prune old messages
        memory::prune_old_messages(&self.db, user_id, 40).await?;

        // Send message end
        if let Some(tx) = &delta_tx {
            let _ = tx.send(StreamEvent::MessageEnd).await;
        }

        Ok(full_response)
    }

    /// Call Anthropic Messages API (non-streaming, with tool definitions)
    async fn call_anthropic(
        &self,
        system: &str,
        messages: &[serde_json::Value],
        tools: &[serde_json::Value],
    ) -> Result<serde_json::Value, AgentError> {
        let api_key = self
            .config
            .anthropic_api_key
            .as_deref()
            .ok_or_else(|| AgentError::Config("ANTHROPIC_API_KEY not set".into()))?;

        let mut body = serde_json::json!({
            "model": self.config.anthropic_model,
            "max_tokens": 4096,
            "system": system,
            "messages": messages,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
        }

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::Api(format!("HTTP error: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AgentError::Api(format!(
                "Anthropic API error ({}): {}",
                status, text
            )));
        }

        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| AgentError::Api(format!("JSON parse error: {}", e)))
    }
}

/// A quick-reply button option sent to the client during intake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickReply {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
}

/// Input type hint for the client — determines which UI widget to show.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    /// Standard quick-reply chips (default)
    Chips,
    /// Multi-select toggle chips with confirm button
    MultiChips,
    /// Native date picker
    DatePicker,
    /// Numeric keyboard
    Number,
    /// Duration/time picker (HH:MM:SS)
    DurationPicker,
    /// Free text input (default text keyboard)
    Text,
}

/// Events streamed to the WebSocket client
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    TextDelta { delta: String },
    ToolUse { tool: String, id: String, input: serde_json::Value },
    ToolResult { id: String, result: String },
    PlanUpdated { plan_id: String, week: Option<u32> },
    QuickReplies {
        question_id: String,
        options: Vec<QuickReply>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_type: Option<InputType>,
    },
    MessageEnd,
    Error { message: String },
}

/// Agent errors
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("API error: {0}")]
    Api(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Tool error: {0}")]
    Tool(String),
}

fn trigger_to_message(trigger: &AgentTrigger) -> String {
    match trigger {
        AgentTrigger::ChatMessage { content } => content.clone(),
        AgentTrigger::StartIntake => "[INTAKE START] Begin het intake-gesprek met de nieuwe gebruiker.".to_string(),
        AgentTrigger::InjuryReport {
            locations,
            severity,
            can_walk,
            can_run,
            description,
        } => {
            format!(
                "[BLESSURE GEMELD] Locatie(s): {}, Ernst: {}/10, Kan lopen: {}, Kan hardlopen: {}{}",
                locations.join(", "),
                severity,
                if *can_walk { "ja" } else { "nee" },
                if *can_run { "ja" } else { "nee" },
                description
                    .as_ref()
                    .map(|d| format!(", Omschrijving: {}", d))
                    .unwrap_or_default()
            )
        }
        AgentTrigger::SessionFeedback {
            week,
            day,
            feeling,
            notes,
            ..
        } => {
            format!(
                "[SESSIE FEEDBACK] Week {}, dag {}, gevoel: {}/5{}",
                week,
                day,
                feeling,
                notes
                    .as_ref()
                    .map(|n| format!(", notitie: {}", n))
                    .unwrap_or_default()
            )
        }
        AgentTrigger::DailyCheckIn => {
            "[DAGELIJKSE CHECK-IN] Geef een korte motiverende update voor vandaag. \
             Check het schema en geef advies voor de geplande training."
                .to_string()
        }
        AgentTrigger::WeekRollover { new_week } => {
            format!(
                "[WEEK OVERGANG] We zijn nu in week {}. Review de vorige week en geef \
                 een preview van de komende week. Pas het plan aan indien nodig.",
                new_week
            )
        }
    }
}

fn trigger_type_name(trigger: &AgentTrigger) -> String {
    match trigger {
        AgentTrigger::ChatMessage { .. } => "chat".into(),
        AgentTrigger::StartIntake => "start_intake".into(),
        AgentTrigger::InjuryReport { .. } => "injury_report".into(),
        AgentTrigger::SessionFeedback { .. } => "session_feedback".into(),
        AgentTrigger::DailyCheckIn => "daily_checkin".into(),
        AgentTrigger::WeekRollover { .. } => "week_rollover".into(),
    }
}
