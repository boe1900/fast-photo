use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: i64, // user id
    pub username: String,
    pub role: String,
    pub exp: usize, // expiry timestamp
    pub iat: usize, // issued at
}

/// Creates a JWT token for a user
pub fn create_token(
    user_id: i64,
    username: &str,
    role: &str,
    secret: &str,
    expiry_hours: u64,
) -> anyhow::Result<String> {
    let now = chrono::Utc::now();
    let exp = now + chrono::Duration::hours(expiry_hours as i64);

    let claims = Claims {
        sub: user_id,
        username: username.to_string(),
        role: role.to_string(),
        exp: exp.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

/// Verifies a JWT token and returns claims
pub fn verify_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

/// Auth extractor for Axum routes  
/// Extracts JWT token from Authorization header and validates it
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: i64,
    pub username: String,
    pub role: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    fn from_request_parts<'life0, 'life1, 'async_trait>(
        parts: &'life0 mut Parts,
        state: &'life1 AppState,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self, Self::Rejection>> + Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let auth_header = parts
                .headers
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    (StatusCode::UNAUTHORIZED, "Missing Authorization header").into_response()
                })?;

            let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
                (StatusCode::UNAUTHORIZED, "Invalid Authorization format").into_response()
            })?;

            let claims = verify_token(token, &state.config.auth.jwt_secret).map_err(|_| {
                (StatusCode::UNAUTHORIZED, "Invalid or expired token").into_response()
            })?;

            Ok(AuthUser {
                user_id: claims.sub,
                username: claims.username,
                role: claims.role,
            })
        })
    }
}
