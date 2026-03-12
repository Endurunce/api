use axum::{
    extract::{Query, State},
    response::Redirect,
    Json,
};
use serde::Deserialize;

use crate::{
    auth,
    db,
    errors::{AppError, ApiResult},
    routes::common::CallbackResponse,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct AuthUrlParams {
    pub state: Option<String>,
}

/// GET /api/auth/google?state=admin — public, returns the Google OAuth URL
pub async fn auth_url(
    State(state): State<AppState>,
    Query(params): Query<AuthUrlParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let client_id = state.config.google_client_id.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GOOGLE_CLIENT_ID not set")))?;
    let redirect_uri = state.config.google_redirect_uri.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GOOGLE_REDIRECT_URI not set")))?;

    let state_param = params.state.unwrap_or_else(|| "app".into());

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid+email+profile&access_type=offline&prompt=consent&state={}",
        client_id,
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&state_param),
    );

    Ok(Json(serde_json::json!({ "auth_url": auth_url })))
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    id: String,
    email: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    picture: Option<String>,
}

/// GET /api/auth/google/callback?code=...&state=... — public
/// Uses OAuth session pattern for all redirects (no tokens in URLs).
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Result<CallbackResponse, AppError> {
    let client_id = state.config.google_client_id.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GOOGLE_CLIENT_ID not set")))?;
    let client_secret = state.config.google_client_secret.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GOOGLE_CLIENT_SECRET not set")))?;
    let redirect_uri = state.config.google_redirect_uri.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GOOGLE_REDIRECT_URI not set")))?;

    // Exchange code for access token
    let token_resp = state.http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id",     client_id),
            ("client_secret", client_secret),
            ("code",          params.code.as_str()),
            ("grant_type",    "authorization_code"),
            ("redirect_uri",  redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Google token exchange failed: {}", e)))?;

    if !token_resp.status().is_success() {
        let body = token_resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("Google token error: {}", body)));
    }

    let token: GoogleTokenResponse = token_resp.json().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Google token parse error: {}", e)))?;

    // Fetch user info
    let user_info_resp = state.http
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Google userinfo failed: {}", e)))?;

    if !user_info_resp.status().is_success() {
        let body = user_info_resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("Google userinfo error: {}", body)));
    }

    let user_info: GoogleUserInfo = user_info_resp.json().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Google userinfo parse error: {}", e)))?;

    let (user_id, email, is_admin, is_new) = db::users::find_or_create_by_google(
        &state.db,
        &user_info.id,
        &user_info.email,
        user_info.name.as_deref(),
        user_info.picture.as_deref(),
    ).await.map_err(AppError::Database)?;

    let jwt = auth::encode_token(user_id, &email, is_admin, &state.config.jwt_secret)?;

    let state_val = params.state.as_deref().unwrap_or("app");

    // Admin panel → OAuth session redirect (no token in URL)
    if state_val == "admin" {
        let session_id = crate::db::oauth_sessions::create(
            &state.db,
            &jwt,
            &email,
            user_info.name.as_deref(),
            is_admin,
            is_new,
        )
        .await
        .map_err(AppError::Database)?;
        let redirect_url = format!("{}/#/oauth?session={}", state.config.admin_url, session_id);
        return Ok(CallbackResponse::Redirect(Redirect::to(&redirect_url)));
    }

    // Flutter web → OAuth session redirect (no token in URL)
    if state_val == "web" {
        let session_id = crate::db::oauth_sessions::create(
            &state.db,
            &jwt,
            &email,
            user_info.name.as_deref(),
            is_admin,
            is_new,
        )
        .await
        .map_err(AppError::Database)?;
        let redirect_url = format!("{}/#/oauth?session={}", state.config.app_url, session_id);
        return Ok(CallbackResponse::Redirect(Redirect::to(&redirect_url)));
    }

    // Mobile app → redirect to custom scheme with OAuth session
    if state_val == "app" {
        let session_id = crate::db::oauth_sessions::create(
            &state.db,
            &jwt,
            &email,
            user_info.name.as_deref(),
            is_admin,
            is_new,
        )
        .await
        .map_err(AppError::Database)?;
        let redirect_url = format!("endurunce://auth?session={}", session_id);
        return Ok(CallbackResponse::Redirect(Redirect::to(&redirect_url)));
    }

    Ok(CallbackResponse::Json(Json(serde_json::json!({
        "token": jwt,
        "user_id": user_id,
        "email": email,
        "is_admin": is_admin,
        "display_name": user_info.name,
        "avatar_url": user_info.picture,
    }))))
}
