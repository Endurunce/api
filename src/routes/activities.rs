use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::Claims,
    db,
    errors::{ApiResult, AppError},
    models::activity::{Activity, ActivityInput},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// POST /api/activities — create an activity (manual or link to session).
pub async fn create_activity(
    State(state): State<AppState>,
    claims: Claims,
    Json(input): Json<ActivityInput>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let id = db::activities::create(&state.db, claims.sub, &input)
        .await
        .map_err(AppError::Database)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id })),
    ))
}

/// GET /api/activities — list activities for the user.
pub async fn list_activities(
    State(state): State<AppState>,
    claims: Claims,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<Vec<Activity>>> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let activities = db::activities::fetch_by_user(&state.db, claims.sub, limit, offset)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(activities))
}

/// GET /api/activities/:id — get a single activity.
pub async fn get_activity(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Activity>> {
    let activity = db::activities::fetch_by_id(&state.db, id, claims.sub)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Activity not found".into()))?;

    Ok(Json(activity))
}
