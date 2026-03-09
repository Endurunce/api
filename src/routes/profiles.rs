use axum::{extract::State, http::StatusCode, Json};

use crate::{
    auth::Claims,
    errors::{AppError, ApiResult},
    AppState,
};

/// GET /api/profiles/me — returns the authenticated user's profile
pub async fn me(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<serde_json::Value>> {
    let row = sqlx::query!(
        r#"
        SELECT name, date_of_birth, gender, race_goal, race_date, terrain,
               weekly_km, running_years
        FROM profiles
        WHERE user_id = $1
        "#,
        claims.sub,
    )
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?;

    match row {
        Some(r) => Ok(Json(serde_json::json!({
            "name":          r.name,
            "date_of_birth": r.date_of_birth,
            "gender":        r.gender,
            "race_goal":     r.race_goal,
            "race_date":     r.race_date,
            "terrain":       r.terrain,
            "weekly_km":     r.weekly_km,
            "running_years": r.running_years,
        }))),
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

/// PATCH /api/profiles/me — update editable personal fields
pub async fn update_me(
    State(state): State<AppState>,
    claims: Claims,
    Json(body): Json<UpdateProfileBody>,
) -> ApiResult<StatusCode> {
    sqlx::query!(
        r#"
        UPDATE profiles SET
            name          = COALESCE($1, name),
            date_of_birth = COALESCE($2, date_of_birth),
            gender        = COALESCE($3, gender),
            weekly_km     = COALESCE($4::float4, weekly_km),
            running_years = COALESCE($5, running_years),
            updated_at    = NOW()
        WHERE user_id = $6
        "#,
        body.name,
        body.date_of_birth,
        body.gender,
        body.weekly_km.map(|v| v as f32),
        body.running_years,
        claims.sub,
    )
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(StatusCode::NO_CONTENT)
}
