use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap},
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AppError;

const TOKEN_EXPIRY_HOURS: i64 = 24 * 7; // 7 days

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,   // user_id
    pub email: String,
    pub exp: usize,
    pub iat: usize,
}

pub fn encode_token(user_id: Uuid, email: &str, secret: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now();
    let exp = now + chrono::Duration::hours(TOKEN_EXPIRY_HOURS);

    let claims = Claims {
        sub: user_id,
        email: email.to_string(),
        iat: now.timestamp() as usize,
        exp: exp.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("JWT encode error: {}", e)))
}

pub fn decode_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::Unauthorized)
}

/// Axum extractor: pulls Claims from the Authorization: Bearer header
#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".into());
        decode_token(bearer.token(), &secret)
    }
}
