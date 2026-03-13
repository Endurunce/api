use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::{
    auth::Claims,
    db,
    errors::{ApiResult, AppError},
    models::injury::{estimated_recovery_weeks, InjuryInput, Injury},
    services::injury as injury_service,
    AppState,
};

/// POST /api/injuries — report a new injury.
pub async fn report_injury(
    State(state): State<AppState>,
    claims: Claims,
    Json(input): Json<InjuryInput>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    // Validate severity
    if input.severity < 1 || input.severity > 10 {
        return Err(AppError::BadRequest("Severity must be between 1 and 10".into()));
    }

    let injury_id = db::injuries::insert(&state.db, claims.sub, &input)
        .await
        .map_err(AppError::Database)?;

    // Try to adapt the active plan
    let plan_adapted = match db::plans::fetch_active(&state.db, claims.sub).await {
        Ok(Some(plan)) => {
            // Build a temporary Injury for adaptation logic
            let injury = Injury {
                id: injury_id,
                user_id: claims.sub,
                locations: input.locations.clone(),
                severity: input.severity,
                can_walk: input.can_walk,
                can_run: input.can_run,
                description: input.description.clone(),
                status: "active".into(),
                reported_at: chrono::Local::now().date_naive(),
                resolved_at: None,
            };
            injury_service::adapt_plan_for_injury(&state.db, &plan, &injury).await.unwrap_or(false)
        }
        _ => false,
    };

    let injury = Injury {
        id: injury_id,
        user_id: claims.sub,
        locations: input.locations,
        severity: input.severity,
        can_walk: input.can_walk,
        can_run: input.can_run,
        description: input.description,
        status: "active".into(),
        reported_at: chrono::Local::now().date_naive(),
        resolved_at: None,
    };
    let recovery_weeks = estimated_recovery_weeks(&injury);

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "injury_id": injury_id,
            "plan_adapted": plan_adapted,
            "recovery_weeks": recovery_weeks,
        })),
    ))
}

/// GET /api/injuries — list active injuries.
pub async fn list_injuries(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<Vec<Injury>>> {
    let injuries = db::injuries::list_active(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?;
    Ok(Json(injuries))
}

/// GET /api/injuries/history — list all injuries.
pub async fn injury_history(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<Vec<Injury>>> {
    let injuries = db::injuries::list_all(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?;
    Ok(Json(injuries))
}

/// PATCH /api/injuries/:id/resolve — resolve an injury.
pub async fn resolve_injury(
    State(state): State<AppState>,
    claims: Claims,
    Path(injury_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let resolved = db::injuries::resolve(&state.db, injury_id, claims.sub)
        .await
        .map_err(AppError::Database)?;

    if resolved {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("Injury not found or already resolved".into()))
    }
}
