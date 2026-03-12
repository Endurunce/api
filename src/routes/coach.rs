use axum::{extract::State, Json};
use serde::Deserialize;

use crate::{
    auth::Claims,
    db,
    errors::{AppError, ApiResult},
    models::plan::{Plan, SessionType},
    services::anthropic,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
}

/// Number of recent messages to include as AI context.
const AI_CONTEXT_MESSAGES: i64 = 20;
/// Maximum number of messages to store per user (older messages are pruned).
const MAX_STORED_MESSAGES: i64 = 60;

/// GET /api/coach — returns the last [`MAX_STORED_MESSAGES`] coach messages.
///
/// **Auth:** Bearer JWT required.
///
/// **Response:** 200 with JSON array of `CoachMessage`.
pub async fn get_messages(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<Vec<db::coach::CoachMessage>>> {
    let messages = db::coach::fetch_messages(&state.db, claims.sub, MAX_STORED_MESSAGES)
        .await
        .map_err(AppError::Database)?;
    Ok(Json(messages))
}

/// POST /api/coach — save user message + get AI response
pub async fn send_message(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<SendMessageRequest>,
) -> ApiResult<Json<db::coach::CoachMessage>> {
    const MAX_CHARS: usize  = 1_000;
    const HOURLY_LIMIT: i64 =     20;
    const DAILY_LIMIT: i64  =    100;

    let text = req.content.trim();

    if text.is_empty() {
        return Err(AppError::BadRequest("Bericht mag niet leeg zijn".into()));
    }
    if text.chars().count() > MAX_CHARS {
        return Err(AppError::BadRequest(
            format!("Bericht is te lang (max {} tekens).", MAX_CHARS)
        ));
    }

    // Rate limiting — check hourly and daily quotas
    let (hourly, daily) = tokio::join!(
        db::coach::count_recent_user_messages(&state.db, claims.sub, 60),
        db::coach::count_recent_user_messages(&state.db, claims.sub, 60 * 24),
    );
    if hourly.map_err(AppError::Database)? >= HOURLY_LIMIT {
        return Err(AppError::TooManyRequests(
            format!("Je hebt het uurlimiet van {} berichten bereikt. Probeer later opnieuw.", HOURLY_LIMIT)
        ));
    }
    if daily.map_err(AppError::Database)? >= DAILY_LIMIT {
        return Err(AppError::TooManyRequests(
            format!("Je hebt het daglimiet van {} berichten bereikt. Morgen kun je weer verder.", DAILY_LIMIT)
        ));
    }

    // Save user turn
    db::coach::save_message(&state.db, claims.sub, "user", text)
        .await
        .map_err(AppError::Database)?;

    // Build context — fetch all data in parallel
    let (history, profile_ctx, plan_opt, injuries) = tokio::join!(
        db::coach::fetch_messages(&state.db, claims.sub, AI_CONTEXT_MESSAGES),
        db::profiles::fetch_full_by_user(&state.db, claims.sub),
        db::plans::fetch_active(&state.db, claims.sub),
        db::injuries::fetch_active(&state.db, claims.sub),
    );

    let history   = history.map_err(AppError::Database)?;
    let profile_ctx = profile_ctx
        .map_err(AppError::Database)?
        .unwrap_or_else(|| "Geen profiel beschikbaar".into());
    let plan_opt  = plan_opt.map_err(AppError::Database)?;
    let injuries  = injuries.map_err(AppError::Database)?;

    let plan_ctx     = build_plan_context(plan_opt.as_ref());
    let injury_ctx   = build_injury_context(&injuries);

    let system = format!(
        "Je bent de EnduRunce Coach — persoonlijke AI-hardloopcoach voor duurlopers.\n\
        \n\
        ## Trainingsfilosofie\n\
        Endurunce hanteert een combinatie van periodisering en hartslaggestuurd trainen (80/20-methode):\n\
        • Schema-niveau: blokopbouw in 4 fasen — Opbouw I → Opbouw II → Piek → Tapering, met elke 4e week een herstelweek.\n\
        • Sessie-niveau: ~80% van de trainingen in Z1-Z2 (aerobe basis, praattempo), ~20% in Z3-Z5 (tempo, interval).\n\
        • Tempo-sessies (Z3) en intervalsessies (Z4-Z5) zijn exact gespecificeerd: geen bandbreedtes, één duidelijke opdracht.\n\
        • Elke sessie in het plan bevat een concrete beschrijving (zie 'Trainingsplan' hieronder).\n\
        \n\
        ## Communicatieregels\n\
        - Spreek de gebruiker aan met je/jij.\n\
        - Geef motiverende, concrete adviezen in het Nederlands.\n\
        - Max 3 alinea's tenzij anders gevraagd.\n\
        - Verwijs naar de hartslag­zones als die beschikbaar zijn in het profiel.\n\
        - Bij blessures: wees voorzichtig en adviseer rust of aanpassing vóór je doorgaat met trainen.\n\
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
            content: m.content.clone(),
        })
        .collect();

    let ai_text = anthropic::complete(&state.http, &state.config, Some(&system), messages, 1024)
        .await
        .unwrap_or_else(|_| "Er is een fout opgetreden. Probeer het opnieuw. 🔄".into());

    // Save assistant turn
    let assistant_msg = db::coach::save_message(&state.db, claims.sub, "assistant", &ai_text)
        .await
        .map_err(AppError::Database)?;

    // Prune old messages
    db::coach::prune_old_messages(&state.db, claims.sub, MAX_STORED_MESSAGES).await.ok();

    Ok(Json(assistant_msg))
}

// ── Context builders ──────────────────────────────────────────────────────────

const DAYS_NL: [&str; 7] = ["Ma", "Di", "Wo", "Do", "Vr", "Za", "Zo"];

fn build_plan_context(plan: Option<&Plan>) -> String {
    let Some(plan) = plan else {
        return "Geen actief trainingsplan.".into();
    };

    let total_weeks = plan.weeks.len();

    // Active (non-rest) days across all weeks
    let total_sessions: usize = plan.weeks.iter()
        .map(|w| w.days.iter().filter(|d| d.session_type != SessionType::Rest).count())
        .sum();
    let completed_sessions: usize = plan.weeks.iter()
        .map(|w| w.days.iter().filter(|d| d.completed).count())
        .sum();

    // Determine current week: first week with uncompleted active days
    let current_week = plan.weeks.iter()
        .find(|w| w.days.iter().any(|d| d.session_type != SessionType::Rest && !d.completed))
        .or_else(|| plan.weeks.last());

    let mut ctx = format!(
        "{} weken totaal | Voortgang: {}/{} sessies afgerond\n",
        total_weeks, completed_sessions, total_sessions
    );

    if let Some(week) = current_week {
        let active_days: Vec<_> = week.days.iter()
            .filter(|d| d.session_type != SessionType::Rest)
            .collect();
        let week_done = active_days.iter().filter(|d| d.completed).count();
        let week_km_done: f32 = active_days.iter()
            .filter(|d| d.completed)
            .map(|d| d.effective_km())
            .sum();

        ctx.push_str(&format!(
            "\nHuidige week: week {} van {} — {} ({})\n\
             Target: {:.0} km | Afgerond: {}/{} sessies, {:.0} km\n\
             Sessies:\n",
            week.week_number, total_weeks,
            week.phase.label(),
            if week.is_recovery { "herstelweek" } else { "trainingsweek" },
            week.target_km,
            week_done, active_days.len(),
            week_km_done,
        ));

        for day in &week.days {
            if day.session_type == SessionType::Rest { continue; }
            let day_name = DAYS_NL.get(day.weekday as usize).unwrap_or(&"?");
            let status = if day.completed { "✓" } else { "·" };
            let km = day.effective_km();
            ctx.push_str(&format!(
                "  {} {}: {} — {:.0} km",
                status, day_name, day.session_type.label(), km
            ));
            if let Some(note) = &day.notes {
                ctx.push_str(&format!(" | {}", note));
            }
            if let Some(fb) = &day.feedback {
                ctx.push_str(&format!(" (gevoel: {}/5{}{})",
                    fb.feeling,
                    if fb.pain { ", pijn gemeld" } else { "" },
                    fb.notes.as_ref().map(|n| format!(", notitie: {}", n)).unwrap_or_default()));
            }
            ctx.push('\n');
        }

        // Recent completed weeks for history context (up to 3 previous)
        let prev_completed: Vec<_> = plan.weeks.iter()
            .filter(|w| {
                w.week_number < week.week_number &&
                w.days.iter().any(|d| d.completed)
            })
            .rev()
            .take(3)
            .collect();

        if !prev_completed.is_empty() {
            ctx.push_str("\nRecente weken:\n");
            for w in prev_completed.iter().rev() {
                let active = w.days.iter().filter(|d| d.session_type != SessionType::Rest).count();
                let done   = w.days.iter().filter(|d| d.completed).count();
                let km: f32 = w.days.iter().filter(|d| d.completed).map(|d| d.effective_km()).sum();
                ctx.push_str(&format!(
                    "  Week {}: {} — {:.0} km, {}/{} sessies{}\n",
                    w.week_number, w.phase.label(), km, done, active,
                    if done == active { " ✓" } else { "" }
                ));
            }
        }
    }

    ctx
}

fn build_injury_context(injuries: &[db::injuries::InjuryRow]) -> String {
    if injuries.is_empty() {
        return "Geen actieve blessures.".into();
    }

    let mut ctx = format!("{} actieve blessure(s):\n", injuries.len());
    for inj in injuries {
        let can_run = if inj.can_run { "kan hardlopen" } else { "kan niet hardlopen" };
        ctx.push_str(&format!(
            "  - Ernst {}/10, {}, status: {}, gemeld: {}",
            inj.severity, can_run, inj.recovery_status, inj.reported_at
        ));
        if let Some(desc) = &inj.description {
            ctx.push_str(&format!(", omschrijving: {}", desc));
        }
        ctx.push('\n');
    }
    ctx
}
