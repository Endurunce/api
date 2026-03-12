use axum::{extract::State, http::StatusCode, Json};
use email_address::EmailAddress;
use serde::{Deserialize, Serialize};

use crate::{
    auth,
    db,
    errors::{AppError, ApiResult},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: uuid::Uuid,
    pub email: String,
    pub is_admin: bool,
}

/// POST /api/auth/register — create a new user account.
///
/// **Auth:** None (public endpoint, rate-limited).
///
/// **Request body:** `{ "email": string, "password": string }` (min 8 chars).
///
/// **Response:** 201 with `AuthResponse { token, user_id, email, is_admin }`.
///
/// **Errors:** 400 if email invalid, password too short, or email already registered.
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, Json<AuthResponse>)> {
    if !EmailAddress::is_valid(&req.email) {
        return Err(AppError::BadRequest("Invalid email".into()));
    }
    if req.password.len() < 8 {
        return Err(AppError::BadRequest("Password must be at least 8 characters".into()));
    }

    let hash = bcrypt::hash(&req.password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt error: {}", e)))?;

    // Rely on DB UNIQUE constraint instead of check-then-insert race condition
    let user_id = match db::users::insert(&state.db, &req.email, &hash).await {
        Ok(id) => id,
        Err(sqlx::Error::Database(e)) if e.constraint() == Some("users_email_key") => {
            return Err(AppError::BadRequest("Email already registered".into()));
        }
        Err(e) => return Err(AppError::Database(e)),
    };

    let token = auth::encode_token(user_id, &req.email, false, &state.config.jwt_secret)?;

    Ok((StatusCode::CREATED, Json(AuthResponse { token, user_id, email: req.email, is_admin: false })))
}

/// POST /api/auth/login — authenticate with email and password.
///
/// **Auth:** None (public endpoint, rate-limited).
///
/// **Request body:** `{ "email": string, "password": string }`.
///
/// **Response:** 200 with `AuthResponse { token, user_id, email, is_admin }`.
///
/// **Errors:** 400 if credentials are invalid.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let user = db::users::fetch_by_email(&state.db, &req.email)
        .await?
        .ok_or_else(|| AppError::BadRequest("Invalid email or password".into()))?;

    let valid = bcrypt::verify(&req.password, &user.password_hash)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt error: {}", e)))?;

    if !valid {
        return Err(AppError::BadRequest("Invalid email or password".into()));
    }

    let is_admin = db::users::fetch_is_admin(&state.db, user.id).await?;

    let token = auth::encode_token(user.id, &user.email, is_admin, &state.config.jwt_secret)?;

    Ok(Json(AuthResponse { token, user_id: user.id, email: user.email, is_admin }))
}
