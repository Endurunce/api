use axum::{extract::State, Json};

use crate::{
    auth,
    db,
    errors::{AppError, ApiResult},
    AppState,
};

/// POST /api/test/oauth-session
/// Maakt een test OAuth sessie aan voor e2e testing.
/// Alleen beschikbaar wanneer TEST_MODE=true.
pub async fn create_oauth_session(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    if std::env::var("TEST_MODE").unwrap_or_default() != "true" {
        return Err(AppError::NotFound("Not found".into()));
    }

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

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());
    let jwt = auth::encode_token(user_id, &email, is_admin, &secret)?;

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
