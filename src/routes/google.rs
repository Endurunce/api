use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    auth,
    db,
    errors::{AppError, ApiResult},
    AppState,
};

/// GET /api/auth/google — public, returns the Google OAuth URL
pub async fn auth_url() -> ApiResult<Json<serde_json::Value>> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("GOOGLE_CLIENT_ID not set")))?;
    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("GOOGLE_REDIRECT_URI not set")))?;

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid+email+profile&access_type=offline&prompt=consent",
        client_id,
        urlencoding::encode(&redirect_uri),
    );

    Ok(Json(serde_json::json!({ "auth_url": auth_url })))
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    #[allow(dead_code)]
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

/// GET /api/auth/google/callback?code=... — public, exchanges code for user info + JWT
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("GOOGLE_CLIENT_ID not set")))?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("GOOGLE_CLIENT_SECRET not set")))?;
    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("GOOGLE_REDIRECT_URI not set")))?;

    let client = reqwest::Client::new();

    // Exchange code for access token
    let token_resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id",     client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code",          params.code.as_str()),
            ("grant_type",    "authorization_code"),
            ("redirect_uri",  redirect_uri.as_str()),
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
    let user_info_resp = client
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

    let (user_id, email, is_admin) = db::users::find_or_create_by_google(
        &state.db,
        &user_info.id,
        &user_info.email,
        user_info.name.as_deref(),
        user_info.picture.as_deref(),
    ).await.map_err(AppError::Database)?;

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());
    let jwt = auth::encode_token(user_id, &email, is_admin, &secret)?;

    Ok(Json(serde_json::json!({
        "token": jwt,
        "user_id": user_id,
        "email": email,
        "is_admin": is_admin,
        "display_name": user_info.name,
        "avatar_url": user_info.picture,
    })))
}
