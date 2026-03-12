use axum::{extract::State, http::StatusCode, Json};

use crate::{
    auth::Claims,
    db,
    errors::{AppError, ApiResult},
    AppState,
};

/// GET /api/profiles/me — returns the authenticated user's profile.
///
/// **Auth:** Bearer JWT required.
///
/// **Response:** 200 with profile JSON, or `null` if no profile exists yet
/// (profile is created when a plan is generated).
pub async fn me(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<serde_json::Value>> {
    let row = db::profiles::fetch_me(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?;

    match row {
        Some(data) => Ok(Json(data)),
        None => Ok(Json(serde_json::Value::Null)),
    }
}

#[derive(serde::Deserialize)]
pub struct UpdateProfileBody {
    pub name:          Option<String>,
    pub date_of_birth: Option<chrono::NaiveDate>,
    pub gender:        Option<String>,
    pub weekly_km:     Option<f64>,
    pub running_years: Option<String>,
}

/// PATCH /api/profiles/me — update editable personal fields.
///
/// **Auth:** Bearer JWT required.
///
/// **Request body:** `{ name?, date_of_birth?, gender?, weekly_km?, running_years? }` (all optional).
///
/// **Response:** 204 No Content.
pub async fn update_me(
    State(state): State<AppState>,
    claims: Claims,
    Json(body): Json<UpdateProfileBody>,
) -> ApiResult<StatusCode> {
    db::profiles::update_me(
        &state.db,
        claims.sub,
        body.name.as_deref(),
        body.date_of_birth,
        body.gender.as_deref(),
        body.weekly_km.map(|v| v as f32),
        body.running_years.as_deref(),
    )
    .await
    .map_err(AppError::Database)?;

    Ok(StatusCode::NO_CONTENT)
}
