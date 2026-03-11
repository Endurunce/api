use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Local;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::Claims,
    db,
    errors::{AppError, ApiResult},
    models::injury::{BodyLocation, InjuryReport, RecoveryStatus},
    services::injury::{adapt_plan_for_injury, estimated_recovery_weeks},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct ReportInjuryRequest {
    pub locations: Vec<BodyLocation>,
    pub severity: u8,
    pub can_walk: bool,
    pub can_run: bool,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReportInjuryResponse {
    pub injury_id: Uuid,
    pub plan_adapted: bool,
    pub recovery_weeks: u8,
}

#[derive(Debug, Serialize)]
pub struct InjuryListItem {
    pub id: Uuid,
    pub severity: i16,
    pub can_run: bool,
    pub recovery_status: String,
    pub reported_at: chrono::NaiveDate,
    pub description: Option<String>,
}

/// POST /api/injuries
pub async fn report_injury(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<ReportInjuryRequest>,
) -> ApiResult<(StatusCode, Json<ReportInjuryResponse>)> {
    if req.severity < 1 || req.severity > 10 {
        return Err(AppError::BadRequest("severity must be between 1 and 10".into()));
    }

    let injury = InjuryReport {
        id: Uuid::new_v4(),
        user_id: claims.sub,
        reported_at: Local::now().date_naive(),
        locations: req.locations,
        severity: req.severity,
        can_walk: req.can_walk,
        can_run: req.can_run,
        description: req.description,
        recovery_status: RecoveryStatus::Active,
    };

    // Fetch active plan with metadata so we can calculate the current week
    let plan_meta = db::plans::fetch_active_with_meta(&state.db, claims.sub).await?;

    let injury_id = db::injuries::insert(
        &state.db,
        &injury,
        plan_meta.as_ref().map(|m| m.plan.id),
    )
    .await?;

    let plan_adapted = if let Some(mut meta) = plan_meta {
        let plan_start = meta.created_at.date_naive();
        let today = Local::now().date_naive();
        let weeks_elapsed = (today - plan_start).num_weeks() as u8;
        let current_week = (weeks_elapsed + 1).clamp(1, meta.plan.weeks.len() as u8);

        adapt_plan_for_injury(&mut meta.plan, &injury, current_week);
        db::plans::update_weeks(&state.db, meta.plan.id, &meta.plan.weeks).await?;
        true
    } else {
        false
    };

    let recovery_weeks = estimated_recovery_weeks(&injury);

    Ok((StatusCode::CREATED, Json(ReportInjuryResponse { injury_id, plan_adapted, recovery_weeks })))
}

/// GET /api/injuries
pub async fn list_injuries(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<Vec<InjuryListItem>>> {
    let rows = db::injuries::fetch_active(&state.db, claims.sub).await?;

    let items = rows.into_iter().map(|r| InjuryListItem {
        id: r.id,
        severity: r.severity,
        can_run: r.can_run,
        recovery_status: r.recovery_status,
        reported_at: r.reported_at,
        description: r.description,
    }).collect();

    Ok(Json(items))
}

#[derive(Debug, Serialize)]
pub struct InjuryHistoryItem {
    pub id: Uuid,
    pub severity: i16,
    pub can_run: bool,
    pub recovery_status: String,
    pub reported_at: chrono::NaiveDate,
    pub resolved_at: Option<chrono::NaiveDate>,
    pub locations: Vec<String>,
    pub description: Option<String>,
}

/// GET /api/injuries/history
pub async fn injury_history(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<Vec<InjuryHistoryItem>>> {
    let rows = db::injuries::fetch_history(&state.db, claims.sub).await?;

    let items = rows.into_iter().map(|r| InjuryHistoryItem {
        id: r.id,
        severity: r.severity,
        can_run: r.can_run,
        recovery_status: r.recovery_status,
        reported_at: r.reported_at,
        resolved_at: r.resolved_at,
        locations: r.locations,
        description: r.description,
    }).collect();

    Ok(Json(items))
}

/// PATCH /api/injuries/:id/resolve
pub async fn resolve_injury(
    State(state): State<AppState>,
    claims: Claims,
    Path(injury_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let updated = db::injuries::resolve_by_user(&state.db, injury_id, claims.sub).await?;

    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("Injury {} not found or already resolved", injury_id)))
    }
}
