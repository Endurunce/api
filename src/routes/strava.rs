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

/// GET /api/auth/strava — public, returns OAuth URL for login/registration (no JWT needed)
pub async fn auth_url() -> ApiResult<Json<StravaConnectResponse>> {
    let client_id = std::env::var("STRAVA_CLIENT_ID")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_ID not set")))?;
    let redirect_uri = std::env::var("STRAVA_REDIRECT_URI")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_REDIRECT_URI not set")))?;

    let auth_url = format!(
        "https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&approval_prompt=auto&scope=activity:read_all&state=login",
        client_id,
        urlencoding::encode(&redirect_uri),
    );

    Ok(Json(StravaConnectResponse { auth_url }))
}

/// GET /api/strava/connect — protected, returns OAuth URL for linking to an existing account
pub async fn connect(claims: Claims) -> ApiResult<Json<StravaConnectResponse>> {
    let client_id = std::env::var("STRAVA_CLIENT_ID")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_ID not set")))?;
    let redirect_uri = std::env::var("STRAVA_REDIRECT_URI")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_REDIRECT_URI not set")))?;

    // Encode user_id in state so callback can link the account
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());
    let state_token = auth::encode_token(claims.sub, &claims.email, claims.is_admin, &secret)?;

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
    #[serde(default)]
    firstname: Option<String>,
    #[serde(default)]
    lastname: Option<String>,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

/// GET /api/strava/callback — handles both login (state="login") and account linking (state=JWT)
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> ApiResult<Json<serde_json::Value>> {
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

    let display_name = match (&token_resp.athlete.firstname, &token_resp.athlete.lastname) {
        (Some(f), Some(l)) => Some(format!("{} {}", f, l)),
        (Some(f), None)    => Some(f.clone()),
        _                  => None,
    };

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());

    // State = "login" → find/create user; state = JWT → link to existing account
    let (user_id, email, is_admin) = if params.state == "login" {
        db::users::find_or_create_by_strava(
            &state.db,
            token_resp.athlete.id,
            token_resp.athlete.email.as_deref(),
            display_name.as_deref(),
            token_resp.athlete.profile.as_deref(),
        ).await.map_err(AppError::Database)?
    } else {
        // Link mode: state is a JWT containing the existing user_id
        let claims = auth::decode_token(&params.state, &secret)?;
        (claims.sub, claims.email, claims.is_admin)
    };

    db::strava::upsert_tokens(
        &state.db,
        user_id,
        token_resp.athlete.id,
        &token_resp.access_token,
        &token_resp.refresh_token,
        expires_at,
        "activity:read_all",
    )
    .await?;

    // If login mode, return a JWT so the client can authenticate
    if params.state == "login" {
        let token = auth::encode_token(user_id, &email, is_admin, &secret)?;
        return Ok(Json(serde_json::json!({
            "token": token,
            "user_id": user_id,
            "email": email,
            "is_admin": is_admin,
            "athlete_id": token_resp.athlete.id,
        })));
    }

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
