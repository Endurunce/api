use axum::{extract::State, Json};

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
        SELECT name, age, gender, race_goal, race_date, terrain,
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
            "age":           r.age,
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
