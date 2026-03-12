use axum::{extract::State, Json};

use crate::{
    auth,
    db,
    errors::{AppError, ApiResult},
    AppState,
};

/// POST /api/test/oauth-session — create a test OAuth session for e2e testing.
///
/// This route is only registered when `TEST_MODE=true` (checked at router build time).
///
/// **Auth:** None (test-only endpoint).
///
/// **Response:** 200 with `{ session_id, email }`.
pub async fn create_oauth_session(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let google_id = format!("test_{}", uuid::Uuid::new_v4());
    let email = format!("e2e+oauth+{}@test.endurunce.app", uuid::Uuid::new_v4());

    let (user_id, email, is_admin, _) = db::users::find_or_create_by_google(
        &state.db,
        &google_id,
        &email,
        Some("E2E Test User"),
        None,
    )
    .await
    .map_err(AppError::Database)?;

    let jwt = auth::encode_token(user_id, &email, is_admin, &state.config.jwt_secret)?;

    let session_id = db::oauth_sessions::create(
        &state.db,
        &jwt,
        &email,
        Some("E2E Test User"),
        is_admin,
        false,
    )
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "email":      email,
    })))
}
