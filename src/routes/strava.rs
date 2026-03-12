use axum::{
    extract::{Query, State},
    response::Redirect,
    Json,
};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{self, Claims},
    db,
    errors::{AppError, ApiResult},
    routes::common::CallbackResponse,
    AppState,
};

#[derive(Debug, Serialize)]
pub struct StravaConnectResponse {
    pub auth_url: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthUrlParams {
    pub state: Option<String>,
}

/// GET /api/auth/strava?state=login|login_web|login_admin — public, returns OAuth URL for login
pub async fn auth_url(
    State(state): State<AppState>,
    Query(params): Query<AuthUrlParams>,
) -> ApiResult<Json<StravaConnectResponse>> {
    let client_id = state.config.strava_client_id.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_ID not set")))?;
    let redirect_uri = state.config.strava_redirect_uri.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("STRAVA_REDIRECT_URI not set")))?;

    let state_val = params.state.unwrap_or_else(|| "login".into());

    let auth_url = format!(
        "https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&approval_prompt=auto&scope=activity:read_all&state={}",
        client_id,
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&state_val),
    );

    Ok(Json(StravaConnectResponse { auth_url }))
}

/// GET /api/strava/connect — protected, returns OAuth URL for linking to an existing account
pub async fn connect(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<StravaConnectResponse>> {
    let client_id = state.config.strava_client_id.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_ID not set")))?;
    let redirect_uri = state.config.strava_redirect_uri.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("STRAVA_REDIRECT_URI not set")))?;

    // Encode user_id in state so callback can link the account
    let state_token = auth::encode_token(claims.sub, &claims.email, claims.is_admin, &state.config.jwt_secret)?;

    let auth_url = format!(
        "https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&approval_prompt=auto&scope=activity:read_all&state={}",
        client_id,
        urlencoding::encode(redirect_uri),
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

/// GET /api/strava/callback — handles login (state=login|login_web|login_admin) and account linking (state=JWT)
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Result<CallbackResponse, AppError> {
    let client_id = state.config.strava_client_id.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_ID not set")))?;
    let client_secret = state.config.strava_client_secret.as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_SECRET not set")))?;

    // Exchange code for tokens
    let resp = state.http
        .post("https://www.strava.com/oauth/token")
        .form(&[
            ("client_id",     client_id),
            ("client_secret", client_secret),
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
        .unwrap_or_else(|| Utc::now() + TimeDelta::hours(6));

    let display_name = match (&token_resp.athlete.firstname, &token_resp.athlete.lastname) {
        (Some(f), Some(l)) => Some(format!("{} {}", f, l)),
        (Some(f), None)    => Some(f.clone()),
        _                  => None,
    };

    let state_val = params.state.as_str();
    let is_login = matches!(state_val, "login" | "login_web" | "login_admin");

    // Login states → find/create user; JWT state → link to existing account
    let (user_id, email, is_admin, is_new) = if is_login {
        db::users::find_or_create_by_strava(
            &state.db,
            token_resp.athlete.id,
            token_resp.athlete.email.as_deref(),
            display_name.as_deref(),
            token_resp.athlete.profile.as_deref(),
        ).await.map_err(AppError::Database)?
    } else {
        let claims = auth::decode_token(state_val, &state.config.jwt_secret)?;
        (claims.sub, claims.email, claims.is_admin, false)
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

    if !is_login {
        return Ok(CallbackResponse::Redirect(Redirect::to(&format!("{}/#/profile", state.config.app_url))));
    }

    let jwt = auth::encode_token(user_id, &email, is_admin, &state.config.jwt_secret)?;

    // Admin panel redirect — use OAuth session pattern (no token in URL)
    if state_val == "login_admin" {
        let session_id = crate::db::oauth_sessions::create(
            &state.db,
            &jwt,
            &email,
            display_name.as_deref(),
            is_admin,
            is_new,
        )
        .await
        .map_err(AppError::Database)?;
        let url = format!("{}/#/oauth?session={}", state.config.admin_url, session_id);
        return Ok(CallbackResponse::Redirect(Redirect::to(&url)));
    }

    // Flutter web redirect — store JWT in a short-lived session, redirect with session ID
    if state_val == "login_web" {
        let session_id = crate::db::oauth_sessions::create(
            &state.db,
            &jwt,
            &email,
            display_name.as_deref(),
            is_admin,
            is_new,
        )
        .await
        .map_err(AppError::Database)?;
        let url = format!("{}/#/oauth?session={}", state.config.app_url, session_id);
        return Ok(CallbackResponse::Redirect(Redirect::to(&url)));
    }

    // Default: JSON for mobile app
    Ok(CallbackResponse::Json(Json(serde_json::json!({
        "token": jwt,
        "user_id": user_id,
        "email": email,
        "is_admin": is_admin,
        "athlete_id": token_resp.athlete.id,
        "display_name": display_name,
    }))))
}

#[derive(Debug, Deserialize)]
pub struct ActivitiesParams {
    pub per_page: Option<u32>,
    pub page: Option<u32>,
}

// ── User-provided credentials flow ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExchangeCodeRequest {
    pub client_id: String,
    pub client_secret: String,
    pub code: String,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExchangeCodeResponse {
    pub athlete_id: i64,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub city: Option<String>,
}

/// POST /api/strava/exchange-code — user provides own Strava credentials
pub async fn exchange_code(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<ExchangeCodeRequest>,
) -> ApiResult<Json<ExchangeCodeResponse>> {
    let mut form = vec![
        ("client_id",     req.client_id.as_str()),
        ("client_secret", req.client_secret.as_str()),
        ("code",          req.code.as_str()),
        ("grant_type",    "authorization_code"),
    ];
    let redirect_uri_owned;
    if let Some(ref uri) = req.redirect_uri {
        redirect_uri_owned = uri.clone();
        form.push(("redirect_uri", redirect_uri_owned.as_str()));
    }

    let resp = state.http
        .post("https://www.strava.com/oauth/token")
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Strava request error: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!("Strava fout: {}", body)));
    }

    let token_resp: StravaTokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Strava parse error: {}", e)))?;

    let expires_at = chrono::DateTime::from_timestamp(token_resp.expires_at, 0)
        .unwrap_or_else(|| Utc::now() + TimeDelta::hours(6));

    let display_name = match (&token_resp.athlete.firstname, &token_resp.athlete.lastname) {
        (Some(f), Some(l)) => Some(format!("{} {}", f, l)),
        (Some(f), None) => Some(f.clone()),
        _ => None,
    };

    // Store token with user's own credentials
    db::strava::upsert_tokens_with_credentials(
        &state.db,
        claims.sub,
        token_resp.athlete.id,
        &token_resp.access_token,
        &token_resp.refresh_token,
        expires_at,
        "activity:read_all",
        &req.client_id,
        &req.client_secret,
        display_name.as_deref(),
        token_resp.athlete.profile.as_deref(),
    )
    .await?;

    Ok(Json(ExchangeCodeResponse {
        athlete_id: token_resp.athlete.id,
        display_name,
        avatar_url: token_resp.athlete.profile,
        city: None,
    }))
}

/// GET /api/strava/status — check if Strava is connected
pub async fn status(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<serde_json::Value>> {
    let tokens = db::strava::fetch_tokens(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?;

    if let Some(t) = tokens {
        let info = db::strava::fetch_athlete_info(&state.db, claims.sub)
            .await
            .map_err(AppError::Database)?;

        Ok(Json(serde_json::json!({
            "connected": true,
            "athlete_id": t.athlete_id,
            "display_name": info.as_ref().and_then(|r| r.display_name.clone()),
            "avatar_url": info.as_ref().and_then(|r| r.avatar_url.clone()),
        })))
    } else {
        Ok(Json(serde_json::json!({ "connected": false })))
    }
}

/// GET /api/strava/activities — fetch recent Strava activities.
///
/// Automatically refreshes the Strava access token if it has expired.
///
/// **Auth:** Bearer JWT required.
///
/// **Query parameters:**
/// - `per_page` (u32, optional, default 20, max 100)
/// - `page` (u32, optional, default 1)
///
/// **Response:** 200 with Strava activities JSON array.
pub async fn activities(
    State(state): State<AppState>,
    claims: Claims,
    Query(params): Query<ActivitiesParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let tokens = db::strava::fetch_tokens(&state.db, claims.sub)
        .await?
        .ok_or_else(|| AppError::BadRequest("Strava not connected. Visit /api/strava/connect first.".into()))?;

    // Auto-refresh if token is expired or about to expire
    let access_token = ensure_fresh_token(&state, claims.sub, &tokens).await?;

    let per_page = params.per_page.unwrap_or(20).min(100);
    let page = params.page.unwrap_or(1);

    let resp = state.http
        .get("https://www.strava.com/api/v3/athlete/activities")
        .bearer_auth(&access_token)
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

// ── Token refresh ─────────────────────────────────────────────────────────────

/// Strava token refresh response.
#[derive(Debug, Deserialize)]
struct StravaRefreshResponse {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

/// Ensure the Strava access token is fresh. If it is expired (or within 5 minutes
/// of expiry), use the stored `refresh_token` to obtain a new `access_token` from
/// the Strava API and persist the updated tokens in the database.
///
/// Returns the valid access token string.
async fn ensure_fresh_token(
    state: &AppState,
    user_id: Uuid,
    tokens: &db::strava::StravaTokenRow,
) -> Result<String, AppError> {
    // Token still valid for > 5 minutes — return as-is
    if tokens.expires_at > Utc::now() + TimeDelta::minutes(5) {
        return Ok(tokens.access_token.clone());
    }

    // Determine which client credentials to use:
    // 1. User-provided credentials stored alongside their tokens
    // 2. Server-level credentials from config
    let (client_id, client_secret) = db::strava::fetch_client_credentials(&state.db, user_id)
        .await
        .map_err(AppError::Database)?
        .map(|(id, secret)| (id, secret))
        .or_else(|| {
            let id = state.config.strava_client_id.clone()?;
            let secret = state.config.strava_client_secret.clone()?;
            Some((id, secret))
        })
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!(
            "No Strava client credentials available for token refresh"
        )))?;

    tracing::info!("Refreshing expired Strava token for user {}", user_id);

    let resp = state.http
        .post("https://www.strava.com/oauth/token")
        .form(&[
            ("client_id",     client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", tokens.refresh_token.as_str()),
            ("grant_type",    "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Strava refresh request failed: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "Strava token refresh failed: {}",
            body
        )));
    }

    let refresh: StravaRefreshResponse = resp.json().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Strava refresh parse error: {}", e)))?;

    let new_expires_at = chrono::DateTime::from_timestamp(refresh.expires_at, 0)
        .unwrap_or_else(|| Utc::now() + TimeDelta::hours(6));

    // Persist new tokens
    db::strava::upsert_tokens(
        &state.db,
        user_id,
        tokens.athlete_id,
        &refresh.access_token,
        &refresh.refresh_token,
        new_expires_at,
        "activity:read_all",
    )
    .await?;

    tracing::info!("Successfully refreshed Strava token for user {}", user_id);

    Ok(refresh.access_token)
}
