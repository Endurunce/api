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
    models::profile::Profile,
    services::schedule::generate_plan,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct GeneratePlanRequest {
    pub profile: Profile,
}

#[derive(Debug, Serialize)]
pub struct GeneratePlanResponse {
    pub plan_id: Uuid,
    pub num_weeks: usize,
    pub plan: crate::models::plan::Plan,
}

/// POST /api/plans/generate — generate a new personalized training plan.
///
/// **Auth:** Bearer JWT required. User must be ≥ 16 years old.
///
/// **Request body:** `{ "profile": Profile }` with race goal, training days, etc.
///
/// **Response:** 201 with `{ plan_id, num_weeks, plan }`. Deactivates any previous plan.
pub async fn generate(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<GeneratePlanRequest>,
) -> ApiResult<(StatusCode, Json<GeneratePlanResponse>)> {
    // Override profile user_id with the authenticated user
    // Always generate a fresh id so client-provided sentinel UUIDs
    // (e.g. ffffffff-ffff-ffff-ffff-ffffffffffff) don't conflict
    // with another user's existing profile row in the primary key.
    let mut profile = req.profile;
    profile.user_id = claims.sub;
    profile.id = Uuid::new_v4();

    // DPIA leeftijdsverificatie: minimumleeftijd 16 jaar (AVG art. 8)
    if profile.age_years() < 16 {
        return Err(AppError::BadRequest(
            "Je moet minimaal 16 jaar oud zijn om deze app te gebruiken.".into(),
        ));
    }

    let plan = generate_plan(&profile);
    let profile_id = db::profiles::upsert(&state.db, &profile).await?;

    let race_date = profile.race_date;
    let race_goal = format!("{:?}", profile.race_goal);

    db::plans::deactivate_all(&state.db, profile.user_id).await?;
    db::plans::insert(&state.db, &plan, profile_id, race_date, &race_goal).await?;

    let num_weeks = plan.weeks.len();
    Ok((StatusCode::CREATED, Json(GeneratePlanResponse { plan_id: plan.id, num_weeks, plan })))
}

/// GET /api/plans — returns the authenticated user's active training plan.
///
/// **Auth:** Bearer JWT required.
///
/// **Response:** 200 with the full `Plan` JSON, or 404 if no active plan exists.
pub async fn get_active(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<crate::models::plan::Plan>> {
    db::plans::fetch_active(&state.db, claims.sub)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound("No active plan found".into()))
}

/// GET /api/plans/:plan_id — returns a specific plan by ID (scoped to the authenticated user).
///
/// **Auth:** Bearer JWT required. Returns 404 if the plan belongs to another user.
///
/// **Response:** 200 with the full `Plan` JSON.
pub async fn get_by_id(
    State(state): State<AppState>,
    claims: Claims,
    Path(plan_id): Path<Uuid>,
) -> ApiResult<Json<crate::models::plan::Plan>> {
    db::plans::fetch_by_id(&state.db, plan_id, claims.sub)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("Plan {} not found", plan_id)))
}
