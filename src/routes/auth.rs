use axum::{extract::State, http::StatusCode, Json};
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
}

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, Json<AuthResponse>)> {
    if req.email.is_empty() || !req.email.contains('@') {
        return Err(AppError::BadRequest("Invalid email".into()));
    }
    if req.password.len() < 8 {
        return Err(AppError::BadRequest("Password must be at least 8 characters".into()));
    }

    if db::users::exists(&state.db, &req.email).await? {
        return Err(AppError::BadRequest("Email already registered".into()));
    }

    let hash = bcrypt::hash(&req.password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt error: {}", e)))?;

    let user_id = db::users::insert(&state.db, &req.email, &hash).await?;

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());
    let token = auth::encode_token(user_id, &req.email, &secret)?;

    Ok((StatusCode::CREATED, Json(AuthResponse { token, user_id, email: req.email })))
}

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

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());
    let token = auth::encode_token(user.id, &user.email, &secret)?;

    Ok(Json(AuthResponse { token, user_id: user.id, email: user.email }))
}
