use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::Claims,
    db,
    errors::{ApiResult, AppError},
    models::plan::GeneratePlanInput,
    services::schedule,
    AppState,
};

/// POST /api/plans/generate — generate a new training plan.
///
/// Accepts a profile JSON (full intake data), generates a normalized plan,
/// and inserts it into plans + plan_weeks + sessions.
pub async fn generate(
    State(state): State<AppState>,
    claims: Claims,
    Json(input): Json<GeneratePlanInput>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    // Parse profile fields from the input
    let profile_val = &input.profile;

    // Validate age
    if let Some(dob_str) = profile_val.get("date_of_birth").and_then(|v| v.as_str()) {
        if let Ok(dob) = dob_str.parse::<chrono::NaiveDate>() {
            let today = chrono::Local::now().date_naive();
            let age = today.year() - dob.year()
                - if today.ordinal() < dob.ordinal() { 1 } else { 0 };
            if age < 16 {
                return Err(AppError::BadRequest(
                    "Je moet minimaal 16 jaar oud zijn om een trainingsplan te genereren.".into(),
                ));
            }
        }
    }

    // Upsert profile
    let profile_input = crate::models::profile::ProfileInput {
        name: profile_val
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Runner")
            .to_string(),
        date_of_birth: profile_val
            .get("date_of_birth")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap()),
        gender: profile_val
            .get("gender")
            .and_then(|v| v.as_str())
            .unwrap_or("other")
            .to_string(),
        running_experience: profile_val
            .get("running_years")
            .or_else(|| profile_val.get("running_experience"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        weekly_km: profile_val
            .get("weekly_km")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32),
        time_5k: profile_val.get("time_5k").and_then(|v| v.as_str()).map(|s| s.to_string()),
        time_10k: profile_val.get("time_10k").and_then(|v| v.as_str()).map(|s| s.to_string()),
        time_half: profile_val.get("time_half_marathon").and_then(|v| v.as_str()).map(|s| s.to_string()),
        time_marathon: profile_val.get("time_marathon").and_then(|v| v.as_str()).map(|s| s.to_string()),
        rest_hr: profile_val.get("rest_hr").and_then(|v| v.as_i64()).map(|v| v as i16),
        max_hr: profile_val.get("max_hr").and_then(|v| v.as_i64()).map(|v| v as i16),
        sleep_quality: profile_val.get("sleep_hours").and_then(|v| v.as_str()).map(|s| s.to_string()),
        complaints: profile_val.get("complaints").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };
    db::profiles::upsert(&state.db, claims.sub, &profile_input)
        .await
        .map_err(AppError::Database)?;

    // Upsert training preferences
    let training_days: Vec<i16> = profile_val
        .get("training_days")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64().map(|n| n as i16)).collect())
        .unwrap_or_else(|| vec![1, 3, 5]);
    let long_run_day = profile_val
        .get("long_run_day")
        .and_then(|v| v.as_i64())
        .map(|v| v as i16);
    let strength_days: Option<Vec<i16>> = profile_val
        .get("strength_days")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64().map(|n| n as i16)).collect());
    let max_dur = profile_val.get("max_duration_per_day").cloned();

    let prefs_input = crate::models::training_preferences::TrainingPreferencesInput {
        training_days: training_days.clone(),
        long_run_day,
        strength_days,
        max_duration_per_day: max_dur,
        terrain: profile_val.get("terrain").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };
    db::training_preferences::upsert(&state.db, claims.sub, &prefs_input)
        .await
        .map_err(AppError::Database)?;

    // Generate the plan using the schedule service
    let race_goal = profile_val
        .get("race_goal")
        .and_then(|v| v.as_str())
        .unwrap_or("marathon")
        .to_string();
    let race_date = profile_val
        .get("race_date")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<chrono::NaiveDate>().ok());
    let race_time_goal = profile_val
        .get("race_time_goal")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let terrain = profile_val
        .get("terrain")
        .and_then(|v| v.as_str())
        .unwrap_or("road")
        .to_string();
    let weekly_km = profile_val
        .get("weekly_km")
        .and_then(|v| v.as_f64())
        .unwrap_or(40.0) as f32;

    let plan_insert = schedule::generate_plan(
        claims.sub,
        &race_goal,
        race_date,
        race_time_goal.as_deref(),
        &terrain,
        weekly_km,
        &training_days,
        long_run_day.unwrap_or(6),
        &profile_input,
    );

    let plan_id = db::plans::insert_full(&state.db, &plan_insert)
        .await
        .map_err(AppError::Database)?;

    // Fetch the plan to return
    let plan = db::plans::fetch_by_id(&state.db, plan_id, claims.sub)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Plan not found after insert".into()))?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "plan_id": plan_id,
            "num_weeks": plan.weeks.len(),
            "plan": plan,
        })),
    ))
}

use chrono::Datelike;

/// GET /api/plans — get the user's active plan.
pub async fn get_active(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<serde_json::Value>> {
    let plan = db::plans::fetch_active(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?;

    match plan {
        Some(p) => Ok(Json(serde_json::to_value(p).unwrap())),
        None => Err(AppError::NotFound("No active plan found".into())),
    }
}

/// GET /api/plans/:plan_id — get a plan by ID.
pub async fn get_by_id(
    State(state): State<AppState>,
    claims: Claims,
    Path(plan_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let plan = db::plans::fetch_by_id(&state.db, plan_id, claims.sub)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound(format!("Plan {} not found", plan_id)))?;

    Ok(Json(serde_json::to_value(plan).unwrap()))
}
