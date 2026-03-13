use axum::{extract::State, Json};
use serde::Deserialize;

use crate::{
    agent::memory,
    auth::Claims,
    db,
    errors::{ApiResult, AppError},
    models::plan::FullPlan,
    services::anthropic,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
}

/// Number of recent messages to include as AI context.
const AI_CONTEXT_MESSAGES: i64 = 20;
/// Maximum number of messages to store per user.
const MAX_STORED_MESSAGES: i64 = 60;

/// GET /api/coach — returns the last messages.
pub async fn get_messages(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<serde_json::Value>> {
    let messages = memory::load_history(&state.db, claims.sub, MAX_STORED_MESSAGES)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

    let result: Vec<serde_json::Value> = messages
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();

    Ok(Json(serde_json::Value::Array(result)))
}

/// POST /api/coach — send a message and get a response.
pub async fn send_message(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<SendMessageRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    const MAX_CHARS: usize = 1_000;

    let text = req.content.trim();

    if text.is_empty() {
        return Err(AppError::BadRequest("Bericht mag niet leeg zijn".into()));
    }
    if text.chars().count() > MAX_CHARS {
        return Err(AppError::BadRequest(
            format!("Bericht is te lang (max {} tekens).", MAX_CHARS),
        ));
    }

    // Save user turn
    memory::save_message(
        &state.db,
        claims.sub,
        "user",
        &serde_json::Value::String(text.to_string()),
    )
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

    // Build context
    let (history, profile_ctx, plan_opt, injuries) = tokio::join!(
        memory::load_history(&state.db, claims.sub, AI_CONTEXT_MESSAGES),
        db::profiles::fetch_full_by_user(&state.db, claims.sub),
        db::plans::fetch_active(&state.db, claims.sub),
        db::injuries::list_active(&state.db, claims.sub),
    );

    let history = history.map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let profile_ctx = profile_ctx
        .map_err(AppError::Database)?
        .unwrap_or_else(|| "Geen profiel beschikbaar".into());
    let plan_opt = plan_opt.map_err(AppError::Database)?;
    let injuries = injuries.map_err(AppError::Database)?;

    let plan_ctx = build_plan_context(plan_opt.as_ref());
    let injury_ctx = build_injury_context(&injuries);

    let system = format!(
        "Je bent de EnduRunce Coach — persoonlijke AI-hardloopcoach.\n\
        \n\
        ## Communicatieregels\n\
        - Spreek de gebruiker aan met je/jij.\n\
        - Geef motiverende, concrete adviezen in het Nederlands.\n\
        - Max 3 alinea's tenzij anders gevraagd.\n\
        - Bij blessures: adviseer rust of aanpassing.\n\
        \n\
        ## Profiel\n{}\n\n\
        ## Trainingsplan\n{}\n\n\
        ## Blessures\n{}",
        profile_ctx, plan_ctx, injury_ctx
    );

    let messages: Vec<anthropic::Message> = history
        .iter()
        .map(|m| anthropic::Message {
            role: m.role.clone(),
            content: m.content.to_string(),
        })
        .collect();

    let ai_text = anthropic::complete(&state.http, &state.config, Some(&system), messages, 1024)
        .await
        .unwrap_or_else(|_| "Er is een fout opgetreden. Probeer het opnieuw. 🔄".into());

    // Save assistant turn
    memory::save_message(
        &state.db,
        claims.sub,
        "assistant",
        &serde_json::Value::String(ai_text.clone()),
    )
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

    // Prune old
    memory::prune_old_messages(&state.db, claims.sub, MAX_STORED_MESSAGES)
        .await
        .ok();

    Ok(Json(serde_json::json!({
        "role": "assistant",
        "content": ai_text,
    })))
}

// ── Context builders ──────────────────────────────────────────────────────────

const DAYS_NL: [&str; 7] = ["Ma", "Di", "Wo", "Do", "Vr", "Za", "Zo"];

fn build_plan_context(plan: Option<&FullPlan>) -> String {
    let Some(plan) = plan else {
        return "Geen actief trainingsplan.".into();
    };

    let total_weeks = plan.weeks.len();
    let mut ctx = format!("{} weken totaal, doel: {}\n", total_weeks, plan.plan.race_goal);

    if let Some(week) = plan.weeks.first() {
        ctx.push_str(&format!(
            "\nWeek {}: {} — target {:.0} km\nSessies:\n",
            week.week.week_number, week.week.phase, week.week.target_km
        ));

        for session in &week.sessions {
            if session.session_type == "rest" {
                continue;
            }
            let day_name = DAYS_NL.get(session.weekday as usize).unwrap_or(&"?");
            ctx.push_str(&format!(
                "  {} {}: {} — {:.0} km",
                "·", day_name, session.session_type, session.target_km
            ));
            if let Some(note) = &session.notes {
                ctx.push_str(&format!(" | {}", note));
            }
            ctx.push('\n');
        }
    }

    ctx
}

fn build_injury_context(injuries: &[crate::models::injury::Injury]) -> String {
    if injuries.is_empty() {
        return "Geen actieve blessures.".into();
    }

    let mut ctx = format!("{} actieve blessure(s):\n", injuries.len());
    for inj in injuries {
        let can_run = if inj.can_run {
            "kan hardlopen"
        } else {
            "kan niet hardlopen"
        };
        ctx.push_str(&format!(
            "  - Ernst {}/10, {}, status: {}, gemeld: {}",
            inj.severity, can_run, inj.status, inj.reported_at
        ));
        if let Some(desc) = &inj.description {
            ctx.push_str(&format!(", omschrijving: {}", desc));
        }
        ctx.push('\n');
    }
    ctx
}
