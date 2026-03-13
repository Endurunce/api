use axum::{
    extract::State,
    http::StatusCode,
    Json,
};

use crate::{
    auth::Claims,
    db,
    errors::{ApiResult, AppError},
    models::profile::ProfilePatch,
    models::training_preferences::TrainingPreferencesInput,
    AppState,
};

/// GET /api/profiles/me — get the current user's profile.
pub async fn me(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<serde_json::Value>> {
    let profile = db::profiles::fetch_by_user(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?;

    match profile {
        Some(p) => Ok(Json(serde_json::to_value(p).unwrap())),
        None => Ok(Json(serde_json::Value::Null)),
    }
}

/// PATCH /api/profiles/me — partial update of the user's profile.
pub async fn update_me(
    State(state): State<AppState>,
    claims: Claims,
    Json(patch): Json<ProfilePatch>,
) -> ApiResult<StatusCode> {
    db::profiles::patch(&state.db, claims.sub, &patch)
        .await
        .map_err(AppError::Database)?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/profiles/me/preferences — get training preferences.
pub async fn get_preferences(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<serde_json::Value>> {
    let prefs = db::training_preferences::fetch_by_user(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?;

    match prefs {
        Some(p) => Ok(Json(serde_json::to_value(p).unwrap())),
        None => Ok(Json(serde_json::Value::Null)),
    }
}

/// PUT /api/profiles/me/preferences — upsert training preferences.
pub async fn update_preferences(
    State(state): State<AppState>,
    claims: Claims,
    Json(input): Json<TrainingPreferencesInput>,
) -> ApiResult<StatusCode> {
    db::training_preferences::upsert(&state.db, claims.sub, &input)
        .await
        .map_err(AppError::Database)?;
    Ok(StatusCode::NO_CONTENT)
}
