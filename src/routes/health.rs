use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::AppState;

pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();

    let status = if db_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "service": "endurance-api",
        "db": if db_ok { "connected" } else { "unreachable" },
    })))
}
