use axum::{
    extract::{Query, State},
    response::Redirect,
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

#[derive(Debug, Deserialize)]
pub struct AuthUrlParams {
    pub state: Option<String>,
}

/// GET /api/auth/strava?state=login|login_web|login_admin — public, returns OAuth URL for login
pub async fn auth_url(Query(params): Query<AuthUrlParams>) -> ApiResult<Json<StravaConnectResponse>> {
    let client_id = std::env::var("STRAVA_CLIENT_ID")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_CLIENT_ID not set")))?;
    let redirect_uri = std::env::var("STRAVA_REDIRECT_URI")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("STRAVA_REDIRECT_URI not set")))?;

    let state_val = params.state.unwrap_or_else(|| "login".into());

    let auth_url = format!(
        "https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&approval_prompt=auto&scope=activity:read_all&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&state_val),
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

pub enum CallbackResponse {
    Json(Json<serde_json::Value>),
    Redirect(Redirect),
}

impl axum::response::IntoResponse for CallbackResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            CallbackResponse::Json(j) => j.into_response(),
            CallbackResponse::Redirect(r) => r.into_response(),
        }
    }
}

/// GET /api/strava/callback — handles login (state=login|login_web|login_admin) and account linking (state=JWT)
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Result<CallbackResponse, AppError> {
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
    let state_val = params.state.as_str();
    let is_login = matches!(state_val, "login" | "login_web" | "login_admin");

    // Login states → find/create user; JWT state → link to existing account
    let (user_id, email, is_admin) = if is_login {
        db::users::find_or_create_by_strava(
            &state.db,
            token_resp.athlete.id,
            token_resp.athlete.email.as_deref(),
            display_name.as_deref(),
            token_resp.athlete.profile.as_deref(),
        ).await.map_err(AppError::Database)?
    } else {
        let claims = auth::decode_token(state_val, &secret)?;
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

    if !is_login {
        return Ok(CallbackResponse::Json(Json(serde_json::json!({
            "connected": true,
            "athlete_id": token_resp.athlete.id,
        }))));
    }

    let jwt = auth::encode_token(user_id, &email, is_admin, &secret)?;

    // Admin panel redirect
    if state_val == "login_admin" {
        let admin_url = std::env::var("ADMIN_URL")
            .unwrap_or_else(|_| "https://admin.endurunce.nl".into());
        let url = format!("{}/#token={}&is_admin={}&email={}", admin_url, jwt, is_admin, urlencoding::encode(&email));
        return Ok(CallbackResponse::Redirect(Redirect::to(&url)));
    }

    // Flutter web redirect
    if state_val == "login_web" {
        let app_url = std::env::var("APP_URL")
            .unwrap_or_else(|_| "https://app.endurunce.nl".into());
        let name_param = display_name.as_deref().map(urlencoding::encode).unwrap_or_default();
        let url = format!("{}/#token={}&is_admin={}&email={}&display_name={}", app_url, jwt, is_admin, urlencoding::encode(&email), name_param);
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
    let client = reqwest::Client::new();
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

    let resp = client
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
        .unwrap_or_else(|| Utc::now() + Duration::hours(6));

    let display_name = match (&token_resp.athlete.firstname, &token_resp.athlete.lastname) {
        (Some(f), Some(l)) => Some(format!("{} {}", f, l)),
        (Some(f), None) => Some(f.clone()),
        _ => None,
    };

    // Store token with user's own credentials
    sqlx::query!(
        r#"
        INSERT INTO strava_tokens (user_id, athlete_id, access_token, refresh_token, expires_at, scope, strava_client_id, strava_client_secret, display_name, avatar_url)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (user_id) DO UPDATE SET
            athlete_id          = EXCLUDED.athlete_id,
            access_token        = EXCLUDED.access_token,
            refresh_token       = EXCLUDED.refresh_token,
            expires_at          = EXCLUDED.expires_at,
            scope               = EXCLUDED.scope,
            strava_client_id    = EXCLUDED.strava_client_id,
            strava_client_secret= EXCLUDED.strava_client_secret,
            display_name        = EXCLUDED.display_name,
            avatar_url          = EXCLUDED.avatar_url,
            updated_at          = NOW()
        "#,
        claims.sub,
        token_resp.athlete.id,
        token_resp.access_token,
        token_resp.refresh_token,
        expires_at,
        "activity:read_all",
        req.client_id,
        req.client_secret,
        display_name,
        token_resp.athlete.profile,
    )
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

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
        // Fetch athlete info from strava_tokens display_name/avatar_url
        let row = sqlx::query!(
            "SELECT display_name, avatar_url, athlete_id FROM strava_tokens WHERE user_id = $1",
            claims.sub,
        )
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?;

        Ok(Json(serde_json::json!({
            "connected": true,
            "athlete_id": t.athlete_id,
            "display_name": row.as_ref().and_then(|r| r.display_name.clone()),
            "avatar_url": row.as_ref().and_then(|r| r.avatar_url.clone()),
        })))
    } else {
        Ok(Json(serde_json::json!({ "connected": false })))
    }
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
