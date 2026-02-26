use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use fast_photo_core::db;
use fast_photo_core::models::{CreateUser, LoginRequest, LoginResponse, UserInfo};

use crate::auth::{create_token, AuthUser};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/me", get(me))
        .route("/setup-status", get(setup_status))
}

/// Check if initial setup is needed (no users exist)
async fn setup_status(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let count = db::get_user_count(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({
        "needs_setup": count == 0,
    })))
}

/// Register a new user (first user becomes admin)
async fn register(
    State(state): State<AppState>,
    Json(body): Json<CreateUser>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Validate input
    if body.username.trim().is_empty() || body.password.len() < 6 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": "Username cannot be empty, password must be at least 6 characters"}),
            ),
        ));
    }

    let user_count = db::get_user_count(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Database error"})),
        )
    })?;

    // First user is admin, rest need to be created by admin
    let role = if user_count == 0 { "admin" } else { "user" };

    // Hash password
    let hashed = hash_password(&body.password).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to hash password"})),
        )
    })?;

    let user = db::create_user(
        &state.db,
        &CreateUser {
            username: body.username.clone(),
            password: body.password.clone(),
            role: Some(role.to_string()),
        },
        &hashed,
    )
    .await
    .map_err(|_| {
        (
            StatusCode::CONFLICT,
            Json(json!({"error": "Username already exists"})),
        )
    })?;

    let token = create_token(
        user.id,
        &user.username,
        &user.role,
        &state.config.auth.jwt_secret,
        state.config.auth.token_expiry_hours,
    )
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to create token"})),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(json!(LoginResponse {
            token,
            user: UserInfo {
                id: user.id,
                username: user.username,
                role: user.role,
            },
        })),
    ))
}

/// Login
async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user = db::get_user_by_username(&state.db, &body.username)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Invalid credentials"})),
            )
        })?;

    // Verify password
    let valid = verify_password(&body.password, &user.password).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Password verification error"})),
        )
    })?;

    if !valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid credentials"})),
        ));
    }

    let token = create_token(
        user.id,
        &user.username,
        &user.role,
        &state.config.auth.jwt_secret,
        state.config.auth.token_expiry_hours,
    )
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to create token"})),
        )
    })?;

    Ok(Json(json!(LoginResponse {
        token,
        user: UserInfo {
            id: user.id,
            username: user.username,
            role: user.role,
        },
    })))
}

/// Get current user info
async fn me(auth: AuthUser) -> Json<Value> {
    Json(json!(UserInfo {
        id: auth.user_id,
        username: auth.username,
        role: auth.role,
    }))
}

fn hash_password(password: &str) -> anyhow::Result<String> {
    use argon2::password_hash::rand_core::OsRng;
    use argon2::{password_hash::SaltString, Argon2, PasswordHasher};

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Hash error: {}", e))?;
    Ok(hash.to_string())
}

fn verify_password(password: &str, hash: &str) -> anyhow::Result<bool> {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};

    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| anyhow::anyhow!("Hash parse error: {}", e))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
