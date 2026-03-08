use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{self, Claims},
    db,
    errors::{AppError, ApiResult},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct StravaConnectResponse {
    pub auth_url: String,
}

/// GET /api/strava/connect — returns the OAuth URL for the client to redirect to
pub async fn connect(claims: Claims) -> ApiResult<Json<StravaConnectResponse>> {
    let client_id = std::env::var("STRAVA_CLIENT_ID")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_ID not set")))?;
    let redirect_uri = std::env::var("STRAVA_REDIRECT_URI")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_REDIRECT_URI not set")))?;

    // Encode user_id in the state parameter using JWT
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());
    let state_token = auth::encode_token(claims.sub, &claims.email, &secret)?;

    let auth_url = format!(
        "https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&approval_prompt=auto&scope=activity:read_all&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        state_token,
    );

    Ok(Json(StravaConnectResponse { auth_url }))
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
struct StravaTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    athlete: StravaAthlete,
}

#[derive(Debug, Deserialize)]
struct StravaAthlete {
    id: i64,
}

/// GET /api/strava/callback?code=...&state=... — public, called by Strava after auth
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> ApiResult<Json<serde_json::Value>> {
    // Recover user_id from the state JWT
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());
    let claims = auth::decode_token(&params.state, &secret)?;

    let client_id = std::env::var("STRAVA_CLIENT_ID")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_ID not set")))?;
    let client_secret = std::env::var("STRAVA_CLIENT_SECRET")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_SECRET not set")))?;

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let resp = client
        .post("https://www.strava.com/oauth/token")
        .form(&[
            ("client_id",     client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code",          params.code.as_str()),
            ("grant_type",    "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Strava token exchange failed: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("Strava error: {}", body)));
    }

    let token_resp: StravaTokenResponse = resp.json().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Strava response parse error: {}", e)))?;

    let expires_at = chrono::DateTime::from_timestamp(token_resp.expires_at, 0)
        .unwrap_or_else(|| Utc::now() + Duration::hours(6));

    db::strava::upsert_tokens(
        &state.db,
        claims.sub,
        token_resp.athlete.id,
        &token_resp.access_token,
        &token_resp.refresh_token,
        expires_at,
        "activity:read_all",
    )
    .await?;

    Ok(Json(serde_json::json!({
        "connected": true,
        "athlete_id": token_resp.athlete.id,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ActivitiesParams {
    pub per_page: Option<u32>,
    pub page: Option<u32>,
}

/// GET /api/strava/activities — fetch recent Strava activities
pub async fn activities(
    State(state): State<AppState>,
    claims: Claims,
    Query(params): Query<ActivitiesParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let tokens = db::strava::fetch_tokens(&state.db, claims.sub)
        .await?
        .ok_or_else(|| AppError::BadRequest("Strava not connected. Visit /api/strava/connect first.".into()))?;

    if tokens.expires_at <= Utc::now() + Duration::minutes(5) {
        return Err(AppError::BadRequest(
            "Strava token expired. Please reconnect via /api/strava/connect.".into()
        ));
    }

    let per_page = params.per_page.unwrap_or(20).min(100);
    let page = params.page.unwrap_or(1);

    let client = reqwest::Client::new();
    let resp = client
        .get("https://www.strava.com/api/v3/athlete/activities")
        .bearer_auth(&tokens.access_token)
        .query(&[("per_page", per_page), ("page", page)])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Strava API error: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("Strava API error: {}", body)));
    }

    let activities: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Strava parse error: {}", e)))?;

    Ok(Json(activities))
}
