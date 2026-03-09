use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    db,
    errors::{AppError, ApiResult},
    AppState,
};

/// GET /api/auth/session/:id — one-time session token exchange
/// Returns JWT after deleting the session (expires in 10 minutes).
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let session = db::oauth_sessions::consume(&state.db, &session_id)
        .await
        .map_err(AppError::Database)?;

    match session {
        Some(s) => Ok(Json(serde_json::json!({
            "token":        s.jwt,
            "email":        s.email,
            "display_name": s.display_name,
            "is_admin":     s.is_admin,
            "is_new":       s.is_new,
        }))),
        None => Err(AppError::NotFound("Session niet gevonden of verlopen".into())),
    }
}
