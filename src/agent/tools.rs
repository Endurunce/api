use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::AgentError;

/// Return the tool definitions for the Anthropic Messages API.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_user_profile",
            "description": "Haal het volledige profiel van de gebruiker op.",
            "input_schema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "get_active_plan",
            "description": "Haal het actieve trainingsplan op met alle weken en sessies.",
            "input_schema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "get_week_schedule",
            "description": "Haal het schema van een specifieke week op.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "week_number": {
                        "type": "integer",
                        "description": "Het weeknummer (1-based)"
                    }
                },
                "required": ["week_number"]
            }
        }),
        json!({
            "name": "get_active_injuries",
            "description": "Haal alle actieve blessures op.",
            "input_schema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "update_session",
            "description": "Pas een sessie aan (type, km, notitie).",
            "input_schema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "UUID van de sessie"
                    },
                    "session_type": {
                        "type": "string",
                        "description": "Nieuw sessietype (optioneel)"
                    },
                    "target_km": {
                        "type": "number",
                        "description": "Nieuwe target km (optioneel)"
                    },
                    "notes": {
                        "type": "string",
                        "description": "Notitie (optioneel)"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Reden voor de aanpassing"
                    }
                },
                "required": ["session_id", "reason"]
            }
        }),
        json!({
            "name": "log_injury",
            "description": "Registreer een nieuwe blessure.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "locations": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Locaties van de blessure"
                    },
                    "severity": {
                        "type": "integer",
                        "description": "Ernst 1-10"
                    },
                    "can_walk": { "type": "boolean" },
                    "can_run": { "type": "boolean" },
                    "description": { "type": "string" }
                },
                "required": ["locations", "severity", "can_walk", "can_run"]
            }
        }),
        json!({
            "name": "log_activity",
            "description": "Log een trainingsactiviteit (koppel optioneel aan een sessie).",
            "input_schema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "UUID van de sessie (optioneel)"
                    },
                    "distance_km": { "type": "number" },
                    "duration_seconds": { "type": "integer" },
                    "feeling": {
                        "type": "integer",
                        "description": "Gevoel 1-5"
                    },
                    "pain": { "type": "boolean" },
                    "notes": { "type": "string" }
                },
                "required": []
            }
        }),
    ]
}

/// Execute a tool call by name and return the result as JSON.
pub async fn execute_tool(
    db: &PgPool,
    user_id: Uuid,
    tool_name: &str,
    input: &Value,
) -> Result<Value, AgentError> {
    match tool_name {
        "get_user_profile" => get_user_profile(db, user_id).await,
        "get_active_plan" => get_active_plan(db, user_id).await,
        "get_week_schedule" => {
            let week = input["week_number"].as_i64().unwrap_or(1) as i16;
            get_week_schedule(db, user_id, week).await
        }
        "get_active_injuries" => get_active_injuries(db, user_id).await,
        "update_session" => {
            let session_id = input["session_id"]
                .as_str()
                .and_then(|s| s.parse::<Uuid>().ok());
            let session_type = input["session_type"].as_str();
            let target_km = input["target_km"].as_f64().map(|v| v as f32);
            let notes = input["notes"].as_str();
            let reason = input["reason"].as_str().unwrap_or("Geen reden");

            match session_id {
                Some(sid) => {
                    update_session(db, user_id, sid, session_type, target_km, notes, reason).await
                }
                None => Ok(json!({ "error": "Ongeldig session_id" })),
            }
        }
        "log_injury" => log_injury(db, user_id, input).await,
        "log_activity" => log_activity(db, user_id, input).await,
        _ => Err(AgentError::Tool(format!("Unknown tool: {}", tool_name))),
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

async fn get_user_profile(db: &PgPool, user_id: Uuid) -> Result<Value, AgentError> {
    let profile = crate::db::profiles::fetch_by_user(db, user_id).await?;
    match profile {
        Some(p) => Ok(serde_json::to_value(&p).unwrap_or(json!({ "error": "serialization" }))),
        None => Ok(json!({ "error": "Geen profiel gevonden" })),
    }
}

async fn get_active_plan(db: &PgPool, user_id: Uuid) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    match plan {
        Some(p) => Ok(serde_json::to_value(&p).unwrap_or(json!({ "error": "serialization" }))),
        None => Ok(json!({ "error": "Geen actief plan gevonden" })),
    }
}

async fn get_week_schedule(
    db: &PgPool,
    user_id: Uuid,
    week_number: i16,
) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    let Some(plan) = plan else {
        return Ok(json!({ "error": "Geen actief plan gevonden" }));
    };

    let week = plan
        .weeks
        .iter()
        .find(|w| w.week.week_number == week_number);
    match week {
        Some(w) => Ok(serde_json::to_value(w).unwrap_or(json!({ "error": "serialization" }))),
        None => Ok(json!({ "error": format!("Week {} niet gevonden", week_number) })),
    }
}

async fn get_active_injuries(db: &PgPool, user_id: Uuid) -> Result<Value, AgentError> {
    let injuries = crate::db::injuries::list_active(db, user_id).await?;
    Ok(serde_json::to_value(&injuries).unwrap_or(json!({ "injuries": [] })))
}

async fn update_session(
    db: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
    session_type: Option<&str>,
    target_km: Option<f32>,
    notes: Option<&str>,
    _reason: &str,
) -> Result<Value, AgentError> {
    let updated = crate::db::plans::update_session(db, session_id, user_id, session_type, target_km, notes)
        .await?;

    if updated {
        Ok(json!({ "success": true, "session_id": session_id.to_string() }))
    } else {
        Ok(json!({ "error": "Sessie niet gevonden of geen toegang" }))
    }
}

async fn log_injury(db: &PgPool, user_id: Uuid, input: &Value) -> Result<Value, AgentError> {
    let locations: Vec<String> = input["locations"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|l| l.as_str().map(|s| s.to_string()))
        .collect();

    let severity = input["severity"].as_i64().unwrap_or(5).clamp(1, 10) as i16;
    let can_walk = input["can_walk"].as_bool().unwrap_or(true);
    let can_run = input["can_run"].as_bool().unwrap_or(true);
    let description = input["description"].as_str().map(String::from);

    let injury_input = crate::models::injury::InjuryInput {
        locations,
        severity,
        can_walk,
        can_run,
        description,
    };

    let injury_id = crate::db::injuries::insert(db, user_id, &injury_input).await?;

    Ok(json!({
        "success": true,
        "injury_id": injury_id.to_string(),
    }))
}

async fn log_activity(db: &PgPool, user_id: Uuid, input: &Value) -> Result<Value, AgentError> {
    let session_id = input["session_id"]
        .as_str()
        .and_then(|s| s.parse::<Uuid>().ok());

    let activity_input = crate::models::activity::ActivityInput {
        session_id,
        source: Some("coach".into()),
        source_id: None,
        activity_type: Some("run".into()),
        distance_km: input["distance_km"].as_f64().map(|v| v as f32),
        duration_seconds: input["duration_seconds"].as_i64().map(|v| v as i32),
        avg_pace_sec_km: None,
        avg_hr: None,
        max_hr: None,
        elevation_m: None,
        calories: None,
        feeling: input["feeling"].as_i64().map(|v| v as i16),
        pain: input["pain"].as_bool(),
        notes: input["notes"].as_str().map(String::from),
        started_at: None,
        completed_at: None,
    };

    let id = crate::db::activities::create(db, user_id, &activity_input).await?;

    Ok(json!({
        "success": true,
        "activity_id": id.to_string(),
    }))
}
