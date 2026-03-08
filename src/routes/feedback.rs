use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::Claims,
    db,
    errors::{AppError, ApiResult},
    models::feedback::{AiAdvice, AlertLevel, PlanAdjustment},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct CompleteDayRequest {
    pub feeling: u8,
    pub pain: bool,
    pub notes: Option<String>,
    pub actual_km: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct CompleteDayResponse {
    pub feedback_id: Uuid,
    pub ai_advice: Option<AiAdvice>,
}

/// POST /api/plans/:plan_id/weeks/:week/days/:weekday/complete
pub async fn complete_day(
    State(state): State<AppState>,
    claims: Claims,
    Path((plan_id, week_number, weekday)): Path<(Uuid, u8, u8)>,
    Json(req): Json<CompleteDayRequest>,
) -> ApiResult<(StatusCode, Json<CompleteDayResponse>)> {
    if req.feeling < 1 || req.feeling > 5 {
        return Err(AppError::BadRequest("feeling must be between 1 and 5".into()));
    }
    if weekday > 6 {
        return Err(AppError::BadRequest("weekday must be between 0 and 6".into()));
    }

    // Fetch plan (scoped to authenticated user)
    let mut plan = db::plans::fetch_by_id(&state.db, plan_id, claims.sub)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Plan {} not found", plan_id)))?;

    // Locate the week
    let week = plan.weeks.iter_mut()
        .find(|w| w.week_number == week_number)
        .ok_or_else(|| AppError::NotFound(format!("Week {} not found in plan", week_number)))?;

    // Locate the day
    let day = week.days.iter_mut()
        .find(|d| d.weekday == weekday)
        .ok_or_else(|| AppError::NotFound(format!("Day {} not found in week {}", weekday, week_number)))?;

    // Mark completed and update actual km
    day.completed = true;
    if let Some(km) = req.actual_km {
        day.adjusted_km = Some(km);
    }

    // Generate AI advice when session was very hard and painful
    let ai_advice: Option<AiAdvice> = if req.pain && req.feeling <= 2 {
        Some(AiAdvice {
            message: "Je rapporteerde pijn en een zeer zware sessie. Overweeg om de belasting volgende week met 20% te verminderen en extra te focussen op herstel.".into(),
            alert_level: AlertLevel::Yellow,
            adjustment: Some(PlanAdjustment {
                reduce_km_percent: Some(20.0),
                convert_to_cross: false,
                insert_rest_days: 0,
                message: "Automatisch gegenereerd advies op basis van sessie-feedback.".into(),
            }),
        })
    } else if req.pain && req.feeling <= 3 {
        Some(AiAdvice {
            message: "Let op: je rapporteerde pijn tijdens deze sessie. Houd dit goed in de gaten en overleg zo nodig met een fysiotherapeut.".into(),
            alert_level: AlertLevel::Yellow,
            adjustment: None,
        })
    } else {
        None
    };

    let ai_advice_json = ai_advice.as_ref()
        .map(|a| serde_json::to_value(a).expect("advice serialization failed"));

    // Persist feedback
    let feedback_id = db::feedback::upsert(
        &state.db,
        claims.sub,
        plan_id,
        week_number as i16,
        weekday as i16,
        req.feeling as i16,
        req.pain,
        req.notes.as_deref(),
        req.actual_km,
        ai_advice_json,
    )
    .await?;

    // Persist updated plan (day marked complete + adjusted_km)
    db::plans::update_weeks(&state.db, plan_id, &plan.weeks).await?;

    Ok((StatusCode::CREATED, Json(CompleteDayResponse { feedback_id, ai_advice })))
}
