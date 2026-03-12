use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::AgentError;

/// Return the tool definitions for the Anthropic API.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_user_profile",
            "description": "Haal het volledige profiel van de gebruiker op (naam, leeftijd, ervaring, doelen, hartslagzones, etc.)",
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
            "description": "Haal alle actieve (niet-opgeloste) blessures op.",
            "input_schema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "get_session_history",
            "description": "Haal de afgeronde trainingssessies op (uit het huidige plan).",
            "input_schema": {
                "type": "object",
                "properties": {
                    "last_n_weeks": {
                        "type": "integer",
                        "description": "Aantal recente weken om op te halen (standaard: 4)"
                    }
                },
                "required": []
            }
        }),
        json!({
            "name": "update_plan_week",
            "description": "Pas een specifieke week in het trainingsplan aan. Wijzig sessietypes, kilometers, of voeg rustdagen toe. Gebruik dit voor individuele weekaanpassingen.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "week_number": {
                        "type": "integer",
                        "description": "Het weeknummer om aan te passen"
                    },
                    "days": {
                        "type": "array",
                        "description": "Lijst van dagaanpassingen",
                        "items": {
                            "type": "object",
                            "properties": {
                                "weekday": {
                                    "type": "integer",
                                    "description": "Dagnummer (0=Ma, 6=Zo)"
                                },
                                "session_type": {
                                    "type": "string",
                                    "enum": ["easy", "tempo", "long", "interval", "hike", "rest", "cross", "race"],
                                    "description": "Nieuw sessietype (optioneel)"
                                },
                                "adjusted_km": {
                                    "type": "number",
                                    "description": "Aangepaste kilometers (optioneel)"
                                },
                                "notes": {
                                    "type": "string",
                                    "description": "Notitie bij de dag (optioneel)"
                                }
                            },
                            "required": ["weekday"]
                        }
                    },
                    "reason": {
                        "type": "string",
                        "description": "Reden voor de aanpassing (wordt gelogd)"
                    }
                },
                "required": ["week_number", "days", "reason"]
            }
        }),
        json!({
            "name": "set_rest_day",
            "description": "Verander een trainingsdag naar een rustdag in een specifieke week.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "week_number": {
                        "type": "integer",
                        "description": "Het weeknummer"
                    },
                    "weekday": {
                        "type": "integer",
                        "description": "Dagnummer (0=Ma, 6=Zo)"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Reden voor de rustdag"
                    }
                },
                "required": ["week_number", "weekday", "reason"]
            }
        }),
        json!({
            "name": "adjust_intensity",
            "description": "Schaal de intensiteit (kilometers) voor een reeks weken. Factor 0.5 = halve intensiteit, 1.5 = 50% meer.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "from_week": {
                        "type": "integer",
                        "description": "Startweek (inclusief)"
                    },
                    "to_week": {
                        "type": "integer",
                        "description": "Eindweek (inclusief)"
                    },
                    "factor": {
                        "type": "number",
                        "description": "Schaalfactor (bijv. 0.7 = 30% minder, 1.2 = 20% meer)"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Reden voor de aanpassing"
                    }
                },
                "required": ["from_week", "to_week", "factor", "reason"]
            }
        }),
        json!({
            "name": "log_injury",
            "description": "Registreer een nieuwe blessure voor de gebruiker.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "locations": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Locaties van de blessure (bijv. ['knee', 'shin'])"
                    },
                    "severity": {
                        "type": "integer",
                        "description": "Ernst 1-10 (1=mild, 10=ernstig)"
                    },
                    "can_walk": {
                        "type": "boolean",
                        "description": "Kan de gebruiker lopen?"
                    },
                    "can_run": {
                        "type": "boolean",
                        "description": "Kan de gebruiker hardlopen?"
                    },
                    "description": {
                        "type": "string",
                        "description": "Beschrijving van de blessure"
                    }
                },
                "required": ["locations", "severity", "can_walk", "can_run"]
            }
        }),
        json!({
            "name": "mark_session_complete",
            "description": "Markeer een trainingssessie als afgerond.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "week_number": {
                        "type": "integer",
                        "description": "Het weeknummer"
                    },
                    "weekday": {
                        "type": "integer",
                        "description": "Dagnummer (0=Ma, 6=Zo)"
                    },
                    "feeling": {
                        "type": "integer",
                        "description": "Gevoel 1-5 (1=zwaar, 5=makkelijk)"
                    },
                    "notes": {
                        "type": "string",
                        "description": "Notities bij de sessie"
                    }
                },
                "required": ["week_number", "weekday"]
            }
        }),
    ]
}

/// Execute a tool call and return the result as JSON.
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
            let week = input["week_number"].as_u64().unwrap_or(1) as u8;
            get_week_schedule(db, user_id, week).await
        }
        "get_active_injuries" => get_active_injuries(db, user_id).await,
        "get_session_history" => {
            let weeks = input["last_n_weeks"].as_u64().unwrap_or(4) as u8;
            get_session_history(db, user_id, weeks).await
        }
        "update_plan_week" => {
            let week = input["week_number"].as_u64().unwrap_or(0) as u8;
            let days = input["days"].as_array().cloned().unwrap_or_default();
            let reason = input["reason"].as_str().unwrap_or("Geen reden opgegeven");
            update_plan_week(db, user_id, week, &days, reason).await
        }
        "set_rest_day" => {
            let week = input["week_number"].as_u64().unwrap_or(0) as u8;
            let weekday = input["weekday"].as_u64().unwrap_or(0) as u8;
            let reason = input["reason"].as_str().unwrap_or("Geen reden opgegeven");
            set_rest_day(db, user_id, week, weekday, reason).await
        }
        "adjust_intensity" => {
            let from = input["from_week"].as_u64().unwrap_or(0) as u8;
            let to = input["to_week"].as_u64().unwrap_or(0) as u8;
            let factor = input["factor"].as_f64().unwrap_or(1.0) as f32;
            let reason = input["reason"].as_str().unwrap_or("Geen reden opgegeven");
            adjust_intensity(db, user_id, from, to, factor, reason).await
        }
        "log_injury" => {
            log_injury(db, user_id, input).await
        }
        "mark_session_complete" => {
            let week = input["week_number"].as_u64().unwrap_or(0) as u8;
            let weekday = input["weekday"].as_u64().unwrap_or(0) as u8;
            let feeling = input["feeling"].as_u64().map(|f| f as u8);
            let notes = input["notes"].as_str();
            mark_session_complete(db, user_id, week, weekday, feeling, notes).await
        }
        _ => Err(AgentError::Tool(format!("Unknown tool: {}", tool_name))),
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

async fn get_user_profile(db: &PgPool, user_id: Uuid) -> Result<Value, AgentError> {
    let profile = crate::db::profiles::fetch_full_by_user(db, user_id).await?;
    match profile {
        Some(ctx) => Ok(json!({ "profile": ctx })),
        None => Ok(json!({ "error": "Geen profiel gevonden" })),
    }
}

async fn get_active_plan(db: &PgPool, user_id: Uuid) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    match plan {
        Some(p) => Ok(serde_json::to_value(&p).unwrap_or(json!({ "error": "serialization failed" }))),
        None => Ok(json!({ "error": "Geen actief plan gevonden" })),
    }
}

async fn get_week_schedule(db: &PgPool, user_id: Uuid, week_number: u8) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    let Some(plan) = plan else {
        return Ok(json!({ "error": "Geen actief plan gevonden" }));
    };

    let week = plan.weeks.iter().find(|w| w.week_number == week_number);
    match week {
        Some(w) => Ok(serde_json::to_value(w).unwrap_or(json!({ "error": "serialization failed" }))),
        None => Ok(json!({ "error": format!("Week {} niet gevonden in plan", week_number) })),
    }
}

async fn get_active_injuries(db: &PgPool, user_id: Uuid) -> Result<Value, AgentError> {
    let injuries = crate::db::injuries::fetch_active(db, user_id).await?;
    let result: Vec<Value> = injuries
        .iter()
        .map(|i| {
            json!({
                "id": i.id.to_string(),
                "severity": i.severity,
                "can_run": i.can_run,
                "status": i.recovery_status,
                "reported_at": i.reported_at.to_string(),
                "description": i.description,
            })
        })
        .collect();
    Ok(json!({ "injuries": result }))
}

async fn get_session_history(db: &PgPool, user_id: Uuid, last_n_weeks: u8) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    let Some(plan) = plan else {
        return Ok(json!({ "error": "Geen actief plan gevonden" }));
    };

    use crate::models::plan::SessionType;

    // Find the current week (first with uncompleted sessions)
    let current_week_num = plan
        .weeks
        .iter()
        .find(|w| {
            w.days
                .iter()
                .any(|d| d.session_type != SessionType::Rest && !d.completed)
        })
        .map(|w| w.week_number)
        .unwrap_or(plan.weeks.last().map(|w| w.week_number).unwrap_or(1));

    let start_week = current_week_num.saturating_sub(last_n_weeks);

    let history: Vec<Value> = plan
        .weeks
        .iter()
        .filter(|w| w.week_number >= start_week && w.week_number <= current_week_num)
        .map(|w| {
            let completed_days: Vec<Value> = w
                .days
                .iter()
                .filter(|d| d.completed)
                .map(|d| {
                    json!({
                        "weekday": d.weekday,
                        "session_type": d.session_type,
                        "target_km": d.target_km,
                        "actual_km": d.effective_km(),
                        "notes": d.notes,
                        "feedback": d.feedback,
                    })
                })
                .collect();

            json!({
                "week_number": w.week_number,
                "phase": w.phase,
                "target_km": w.target_km,
                "completed_sessions": completed_days,
            })
        })
        .collect();

    Ok(json!({ "session_history": history }))
}

async fn update_plan_week(
    db: &PgPool,
    user_id: Uuid,
    week_number: u8,
    day_changes: &[Value],
    reason: &str,
) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    let Some(mut plan) = plan else {
        return Ok(json!({ "error": "Geen actief plan gevonden" }));
    };

    let week = plan.weeks.iter_mut().find(|w| w.week_number == week_number);
    let Some(week) = week else {
        return Ok(json!({ "error": format!("Week {} niet gevonden", week_number) }));
    };

    // Capture before state
    let before_state = serde_json::to_value(&*week).unwrap_or_default();

    // Apply changes
    for change in day_changes {
        let weekday = change["weekday"].as_u64().unwrap_or(255) as u8;
        if let Some(day) = week.days.iter_mut().find(|d| d.weekday == weekday) {
            if let Some(st) = change["session_type"].as_str() {
                if let Ok(session_type) = serde_json::from_value(json!(st)) {
                    day.session_type = session_type;
                }
            }
            if let Some(km) = change["adjusted_km"].as_f64() {
                day.adjusted_km = Some(km as f32);
            }
            if let Some(notes) = change["notes"].as_str() {
                day.notes = Some(notes.to_string());
            }
        }
    }

    // Recalculate week target_km
    let new_target: f32 = week.days.iter().map(|d| d.effective_km()).sum();
    week.target_km = new_target;

    // Capture after state
    let after_state = serde_json::to_value(&*week).unwrap_or_default();

    // Save to DB
    crate::db::plans::update_weeks(db, plan.id, &plan.weeks).await?;

    // Log plan change
    log_plan_change(db, plan.id, user_id, "update_week", Some(week_number as i32), &before_state, &after_state, reason).await?;

    Ok(json!({
        "success": true,
        "week": week_number,
        "new_target_km": new_target,
        "reason": reason,
    }))
}

async fn set_rest_day(
    db: &PgPool,
    user_id: Uuid,
    week_number: u8,
    weekday: u8,
    reason: &str,
) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    let Some(mut plan) = plan else {
        return Ok(json!({ "error": "Geen actief plan gevonden" }));
    };

    let week_idx = plan.weeks.iter().position(|w| w.week_number == week_number);
    let Some(week_idx) = week_idx else {
        return Ok(json!({ "error": format!("Week {} niet gevonden", week_number) }));
    };

    let before_state = serde_json::to_value(&plan.weeks[week_idx]).unwrap_or_default();

    let day_found = if let Some(day) = plan.weeks[week_idx].days.iter_mut().find(|d| d.weekday == weekday) {
        let old_type = format!("{:?}", day.session_type);
        day.session_type = crate::models::plan::SessionType::Rest;
        day.adjusted_km = Some(0.0);
        day.notes = Some(format!("Rustdag (was: {}). Reden: {}", old_type, reason));
        true
    } else {
        false
    };

    if !day_found {
        return Ok(json!({ "error": format!("Dag {} niet gevonden in week {}", weekday, week_number) }));
    }

    // Recalculate week target
    plan.weeks[week_idx].target_km = plan.weeks[week_idx].days.iter().map(|d| d.effective_km()).sum();

    let after_state = serde_json::to_value(&plan.weeks[week_idx]).unwrap_or_default();
    let plan_id = plan.id;
    crate::db::plans::update_weeks(db, plan_id, &plan.weeks).await?;
    log_plan_change(db, plan_id, user_id, "set_rest_day", Some(week_number as i32), &before_state, &after_state, reason).await?;

    Ok(json!({
        "success": true,
        "week": week_number,
        "weekday": weekday,
        "reason": reason,
    }))
}

async fn adjust_intensity(
    db: &PgPool,
    user_id: Uuid,
    from_week: u8,
    to_week: u8,
    factor: f32,
    reason: &str,
) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    let Some(mut plan) = plan else {
        return Ok(json!({ "error": "Geen actief plan gevonden" }));
    };

    // Clamp factor to reasonable range
    let factor = factor.clamp(0.3, 2.0);

    let mut weeks_adjusted = 0;
    for week in plan.weeks.iter_mut() {
        if week.week_number >= from_week && week.week_number <= to_week {
            let before_state = serde_json::to_value(&*week).unwrap_or_default();

            for day in week.days.iter_mut() {
                if day.session_type.is_running() {
                    let current_km = day.effective_km();
                    day.adjusted_km = Some((current_km * factor).round());
                }
            }
            week.target_km = week.days.iter().map(|d| d.effective_km()).sum();
            week.week_adjustment = factor;

            let after_state = serde_json::to_value(&*week).unwrap_or_default();
            log_plan_change(
                db, plan.id, user_id, "adjust_intensity",
                Some(week.week_number as i32), &before_state, &after_state, reason,
            )
            .await?;

            weeks_adjusted += 1;
        }
    }

    crate::db::plans::update_weeks(db, plan.id, &plan.weeks).await?;

    Ok(json!({
        "success": true,
        "weeks_adjusted": weeks_adjusted,
        "factor": factor,
        "from_week": from_week,
        "to_week": to_week,
        "reason": reason,
    }))
}

async fn log_injury(
    db: &PgPool,
    user_id: Uuid,
    input: &Value,
) -> Result<Value, AgentError> {
    use crate::models::injury::{BodyLocation, InjuryReport, RecoveryStatus};
    use chrono::Local;

    let locations: Vec<BodyLocation> = input["locations"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|l| {
            let s = l.as_str()?;
            match s.to_lowercase().as_str() {
                "knee" => Some(BodyLocation::Knee),
                "achilles" => Some(BodyLocation::Achilles),
                "shin" => Some(BodyLocation::Shin),
                "hip" => Some(BodyLocation::Hip),
                "hamstring" => Some(BodyLocation::Hamstring),
                "calf" => Some(BodyLocation::Calf),
                "foot" => Some(BodyLocation::Foot),
                "ankle" => Some(BodyLocation::Ankle),
                "lower_back" | "lowerback" => Some(BodyLocation::LowerBack),
                other => Some(BodyLocation::Other(other.to_string())),
            }
        })
        .collect();

    let severity = input["severity"].as_u64().unwrap_or(5).clamp(1, 10) as u8;
    let can_walk = input["can_walk"].as_bool().unwrap_or(true);
    let can_run = input["can_run"].as_bool().unwrap_or(true);
    let description = input["description"].as_str().map(String::from);

    let injury = InjuryReport {
        id: Uuid::new_v4(),
        user_id,
        reported_at: Local::now().date_naive(),
        locations,
        severity,
        can_walk,
        can_run,
        description,
        recovery_status: RecoveryStatus::Active,
    };

    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    let plan_id = plan.as_ref().map(|p| p.id);

    let injury_id = crate::db::injuries::insert(db, &injury, plan_id).await?;

    Ok(json!({
        "success": true,
        "injury_id": injury_id.to_string(),
        "severity": severity,
        "severity_class": format!("{:?}", injury.severity_class()),
    }))
}

async fn mark_session_complete(
    db: &PgPool,
    user_id: Uuid,
    week_number: u8,
    weekday: u8,
    feeling: Option<u8>,
    notes: Option<&str>,
) -> Result<Value, AgentError> {
    let plan = crate::db::plans::fetch_active(db, user_id).await?;
    let Some(mut plan) = plan else {
        return Ok(json!({ "error": "Geen actief plan gevonden" }));
    };

    let week_idx = plan.weeks.iter().position(|w| w.week_number == week_number);
    let Some(week_idx) = week_idx else {
        return Ok(json!({ "error": format!("Week {} niet gevonden", week_number) }));
    };

    let day_idx = plan.weeks[week_idx].days.iter().position(|d| d.weekday == weekday);
    let Some(day_idx) = day_idx else {
        return Ok(json!({ "error": format!("Dag {} niet gevonden in week {}", weekday, week_number) }));
    };

    plan.weeks[week_idx].days[day_idx].completed = true;
    if let Some(n) = notes {
        plan.weeks[week_idx].days[day_idx].notes = Some(n.to_string());
    }

    let effective_km = plan.weeks[week_idx].days[day_idx].effective_km();
    let session_type = plan.weeks[week_idx].days[day_idx].session_type.clone();

    // Save feedback if feeling provided
    if let Some(f) = feeling {
        crate::db::feedback::upsert(
            db,
            user_id,
            plan.id,
            week_number as i16,
            weekday as i16,
            f as i16,
            false,
            notes,
            Some(effective_km),
            None,
        )
        .await?;
    }

    crate::db::plans::update_weeks(db, plan.id, &plan.weeks).await?;

    Ok(json!({
        "success": true,
        "week": week_number,
        "weekday": weekday,
        "session_type": session_type,
        "km": effective_km,
    }))
}

// ── Plan change logging ───────────────────────────────────────────────────────

async fn log_plan_change(
    db: &PgPool,
    plan_id: Uuid,
    user_id: Uuid,
    change_type: &str,
    week_number: Option<i32>,
    before_state: &Value,
    after_state: &Value,
    reason: &str,
) -> Result<(), AgentError> {
    sqlx::query(
        r#"
        INSERT INTO plan_changes (plan_id, user_id, change_type, week_number, before_state, after_state, reason)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(plan_id)
    .bind(user_id)
    .bind(change_type)
    .bind(week_number)
    .bind(before_state)
    .bind(after_state)
    .bind(reason)
    .execute(db)
    .await?;

    Ok(())
}
