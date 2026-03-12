use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{errors::AppError, AppState};

pub const TOKEN_EXPIRY_HOURS: i64 = 24 * 7; // 7 days

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub email: String,
    pub exp: usize,
    pub iat: usize,
    #[serde(default)]
    pub is_admin: bool,
}

/// Wrapper extractor that additionally requires is_admin == true
pub struct AdminClaims(pub Claims);

pub fn encode_token(user_id: Uuid, email: &str, is_admin: bool, secret: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now();
    let exp = now + chrono::TimeDelta::hours(TOKEN_EXPIRY_HOURS);

    let claims = Claims {
        sub: user_id,
        email: email.to_string(),
        is_admin,
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

/// Axum extractor: pulls Claims from the Authorization: Bearer header.
/// Reads JWT_SECRET from AppState config instead of env var.
#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let app_state = AppState::from_ref(state);
        decode_token(bearer.token(), &app_state.config.jwt_secret)
    }
}

/// Axum extractor: same as Claims but rejects non-admin users
#[async_trait]
impl<S> FromRequestParts<S> for AdminClaims
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let claims = Claims::from_request_parts(parts, state).await?;
        if !claims.is_admin {
            return Err(AppError::Forbidden);
        }
        Ok(AdminClaims(claims))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn roundtrip_regular_token() {
        let id = Uuid::new_v4();
        let token = encode_token(id, "runner@example.com", false, "test_secret").unwrap();
        let claims = decode_token(&token, "test_secret").unwrap();
        assert_eq!(claims.sub, id);
        assert_eq!(claims.email, "runner@example.com");
        assert!(!claims.is_admin);
    }

    #[test]
    fn roundtrip_admin_token() {
        let id = Uuid::new_v4();
        let token = encode_token(id, "admin@example.com", true, "test_secret").unwrap();
        let claims = decode_token(&token, "test_secret").unwrap();
        assert!(claims.is_admin);
        assert_eq!(claims.email, "admin@example.com");
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, "runner@example.com", false, "secret_a").unwrap();
        assert!(decode_token(&token, "secret_b").is_err());
    }

    #[test]
    fn tampered_token_is_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, "runner@example.com", false, "secret").unwrap();
        let tampered = format!("{}xyz", token);
        assert!(decode_token(&tampered, "secret").is_err());
    }

    #[test]
    fn empty_token_is_rejected() {
        assert!(decode_token("", "secret").is_err());
    }

    #[test]
    fn token_expiry_is_in_future() {
        let id = Uuid::new_v4();
        let token = encode_token(id, "runner@example.com", false, "secret").unwrap();
        let claims = decode_token(&token, "secret").unwrap();
        let now = chrono::Utc::now().timestamp() as usize;
        assert!(claims.exp > now, "token should expire in the future");
        assert!(claims.iat <= now, "iat should not be in the future");
    }

    #[test]
    fn expiry_is_approximately_seven_days() {
        let id = Uuid::new_v4();
        let token = encode_token(id, "runner@example.com", false, "secret").unwrap();
        let claims = decode_token(&token, "secret").unwrap();
        let expected_duration = TOKEN_EXPIRY_HOURS * 3600;
        let actual_duration = (claims.exp - claims.iat) as i64;
        assert!((actual_duration - expected_duration).abs() < 5, "token expiry should be ~7 days");
    }
}
