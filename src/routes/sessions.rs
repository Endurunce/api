use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::Claims,
    db,
    errors::{ApiResult, AppError},
    models::plan::session_type_label,
    services::anthropic,
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionAdvice {
    pub goal: String,
    pub warmup: String,
    pub main_set: String,
    pub cooldown: String,
    pub summary: String,
    pub go_signal: String,
    pub stop_signal: String,
    pub too_hard: String,
    pub why_now: String,
}

/// GET /api/plans/:plan_id/weeks/:week/days/:weekday/advice — get AI session advice.
pub async fn session_advice(
    State(state): State<AppState>,
    claims: Claims,
    Path((plan_id, week_number, weekday)): Path<(Uuid, i16, i16)>,
) -> ApiResult<Json<SessionAdvice>> {
    let plan = db::plans::fetch_by_id(&state.db, plan_id, claims.sub)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound(format!("Plan {} not found", plan_id)))?;

    let week = plan
        .weeks
        .iter()
        .find(|w| w.week.week_number == week_number)
        .ok_or_else(|| AppError::NotFound("Week niet gevonden".into()))?;

    let session = week
        .sessions
        .iter()
        .find(|s| s.weekday == weekday)
        .ok_or_else(|| AppError::NotFound("Sessie niet gevonden".into()))?;

    let label = session_type_label(&session.session_type);
    let target_km = session.target_km;
    let phase = &week.week.phase;
    let notes_ctx = session.notes.as_deref().unwrap_or("");

    let profile_ctx = db::profiles::fetch_full_by_user(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?
        .unwrap_or_else(|| "Standaard hardloper".into());

    let prescription_line = if notes_ctx.is_empty() {
        String::new()
    } else {
        format!("Trainingsvoorschrift: {notes_ctx}\n")
    };

    let prompt = format!(
        "Genereer een exact trainingsvoorschrift voor deze sessie als JSON.\n\
        Sessie: {label}\n\
        Afstand: {target_km:.1} km\n\
        Fase: {phase}\n\
        Week {week_number} van {num_weeks}\n\
        {prescription_line}\
        Profiel: {profile_ctx}\n\
        \n\
        REGELS:\n\
        - Gebruik NOOIT bereiken zoals '6-8' of '3-4 min'. Kies altijd \u{00e9}\u{00e9}n exact getal.\n\
        - Geef ALLEEN valide JSON terug, geen markdown.\n\
        \n\
        {{\"goal\":\"\",\"warmup\":\"\",\"main_set\":\"\",\"cooldown\":\"\",\"summary\":\"\",\"go_signal\":\"\",\"stop_signal\":\"\",\"too_hard\":\"\",\"why_now\":\"\"}}",
        num_weeks = plan.weeks.len(),
    );

    let messages = vec![anthropic::Message {
        role: "user".into(),
        content: prompt,
    }];

    match anthropic::complete(&state.http, &state.config, None, messages, 512).await {
        Ok(response) => {
            let start = response.find('{').unwrap_or(0);
            let end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
            if let Ok(advice) = serde_json::from_str::<SessionAdvice>(&response[start..end]) {
                return Ok(Json(advice));
            }
        }
        Err(e) => {
            tracing::warn!("AI session advice failed: {}", e);
        }
    }

    Ok(Json(fallback_advice(&session.session_type, target_km, week_number)))
}

/// POST /api/plans/:plan_id/weeks/:week/days/:weekday/complete — mark session complete via activity.
pub async fn complete_session(
    State(state): State<AppState>,
    claims: Claims,
    Path((plan_id, week_number, weekday)): Path<(Uuid, i16, i16)>,
    Json(input): Json<CompleteInput>,
) -> ApiResult<Json<serde_json::Value>> {
    // Find the session
    let sessions = db::plans::fetch_week_sessions(&state.db, plan_id, week_number, claims.sub)
        .await
        .map_err(AppError::Database)?;

    let session = sessions
        .iter()
        .find(|s| s.weekday == weekday)
        .ok_or_else(|| AppError::NotFound("Session not found".into()))?;

    // Create an activity linked to this session
    let activity_input = crate::models::activity::ActivityInput {
        session_id: Some(session.id),
        source: Some("manual".into()),
        source_id: None,
        activity_type: Some("run".into()),
        distance_km: input.actual_km,
        duration_seconds: input.duration_seconds,
        avg_pace_sec_km: None,
        avg_hr: None,
        max_hr: None,
        elevation_m: None,
        calories: None,
        feeling: input.feeling,
        pain: input.pain,
        notes: input.notes.clone(),
        started_at: None,
        completed_at: None,
    };

    let activity_id = db::activities::create(&state.db, claims.sub, &activity_input)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "completed": true,
        "activity_id": activity_id,
    })))
}

/// POST /api/plans/:plan_id/weeks/:week/days/:weekday/uncomplete — undo session completion.
pub async fn uncomplete_session(
    State(state): State<AppState>,
    _claims: Claims,
    Path((_plan_id, _week_number, _weekday)): Path<(Uuid, i16, i16)>,
) -> ApiResult<Json<serde_json::Value>> {
    // In the new model, uncompleting means deleting the activity.
    // For now, just return success — the activity remains for audit.
    Ok(Json(serde_json::json!({ "uncompleted": true })))
}

#[derive(Debug, Deserialize)]
pub struct CompleteInput {
    pub actual_km: Option<f32>,
    pub duration_seconds: Option<i32>,
    pub feeling: Option<i16>,
    pub pain: Option<bool>,
    pub notes: Option<String>,
}

fn fallback_advice(session_type: &str, km: f32, week: i16) -> SessionAdvice {
    match session_type {
        "easy" => SessionAdvice {
            goal: "Aerobe basis opbouwen en herstel bevorderen.".into(),
            warmup: "1 km rustig wandelen, dan geleidelijk optempo naar Z2.".into(),
            main_set: format!("{:.1} km op praattempo Z2.", (km - 1.0_f32).max(1.0)),
            cooldown: "Laatste 500m uitlopen, 5 min stretchen.".into(),
            summary: format!("{:.1} km easy Z2", km),
            go_signal: "Ademhaling rustig, hartslag < 75% max.".into(),
            stop_signal: "Pijn in gewrichten, hartslag > 85%.".into(),
            too_hard: "Verlaag tempo met 30s/km.".into(),
            why_now: format!("Week {} bouwt de aerobe basis.", week),
        },
        "interval" => SessionAdvice {
            goal: "VO2max verhogen via korte intensieve herhalingen.".into(),
            warmup: "2 km rustig inlopen, 4× 100m versnelling.".into(),
            main_set: format!("5× 1000m op 10k-tempo. 90s herstel. Totaal: {:.1} km.", km),
            cooldown: "1.5 km uitlopen.".into(),
            summary: "5× 1000m op 10k-tempo · 90s herstel".into(),
            go_signal: "Consistent tempo per herhaling.".into(),
            stop_signal: "Kniepijn of hartslag > 95% na herstel.".into(),
            too_hard: "4 herhalingen of 2 min herstel.".into(),
            why_now: "Intervaltraining verhoogt je lactaatdrempel.".into(),
        },
        "tempo" => SessionAdvice {
            goal: "Lactaatdrempel verhogen.".into(),
            warmup: "2 km rustig inlopen Z1-Z2.".into(),
            main_set: format!("{:.1} km tempolopen Z3.", (km - 2.0_f32).max(1.0)),
            cooldown: "1 km uitlopen Z1.".into(),
            summary: format!("{:.1} km tempo Z3", km),
            go_signal: "Ademhaling gecontroleerd.".into(),
            stop_signal: "Pijn of geen woord meer.".into(),
            too_hard: "Verlaag naar Z2 of 1 km minder.".into(),
            why_now: "Tempolopen verhoogt je comfortabele racetempo.".into(),
        },
        "long" => SessionAdvice {
            goal: "Uithoudingsvermogen opbouwen.".into(),
            warmup: "Eerste 2 km rustig Z1.".into(),
            main_set: format!("{:.1} km duurloop Z2. Elke 10 km voeding.", (km - 2.0_f32).max(1.0)),
            cooldown: "Laatste km uitlopen, 10 min stretchen.".into(),
            summary: format!("{:.1} km lange duurloop Z2", km),
            go_signal: "Na de helft gelijkmatig tempo.".into(),
            stop_signal: "Uitputting of spierkramp.".into(),
            too_hard: "10% minder of wissel naar wandelen.".into(),
            why_now: "Lange duurloop is de pijler van je week.".into(),
        },
        "rest" => SessionAdvice {
            goal: "Herstel en adaptatie.".into(),
            warmup: "Geen.".into(),
            main_set: "Rust! Stretching of wandeling < 30 min.".into(),
            cooldown: "Goede slaap en hydratatie.".into(),
            summary: "Rustdag — herstel is training.".into(),
            go_signal: "Goed uitgerust.".into(),
            stop_signal: "Forceer niets.".into(),
            too_hard: "Rust IS de aanpassing.".into(),
            why_now: "Rustdagen zijn waar adaptatie plaatsvindt.".into(),
        },
        _ => SessionAdvice {
            goal: format!("{} sessie.", session_type),
            warmup: "10 min opwarmen.".into(),
            main_set: format!("{:.1} km {}.", km, session_type),
            cooldown: "10 min afkoelen.".into(),
            summary: format!("{:.1} km {}", km, session_type),
            go_signal: "Goed gevoel.".into(),
            stop_signal: "Pijn of extreme vermoeidheid.".into(),
            too_hard: "Verlaag intensiteit.".into(),
            why_now: format!("Week {} training.", week),
        },
    }
}
