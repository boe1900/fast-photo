use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use fast_photo_core::db;
use fast_photo_core::models::{CreateUser, LoginRequest, LoginResponse, UserInfo};

use crate::auth::{create_token, AuthUser};
use crate::state::AppState;

const DEFAULT_LOGIN_LIMIT_WINDOW_SECS: u64 = 300;
const DEFAULT_LOGIN_MAX_FAILURES: usize = 10;
const LOGIN_LIMIT_WINDOW_ENV: &str = "FASTPHOTO_LOGIN_LIMIT_WINDOW_SECS";
const LOGIN_MAX_FAILURES_ENV: &str = "FASTPHOTO_LOGIN_MAX_FAILURES";
static LOGIN_FAILURES: OnceLock<Mutex<HashMap<String, Vec<Instant>>>> = OnceLock::new();
static LOGIN_LIMIT_WINDOW_OVERRIDE: OnceLock<Duration> = OnceLock::new();
static LOGIN_MAX_FAILURES_OVERRIDE: OnceLock<usize> = OnceLock::new();

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

    if let Err(e) = db::log_activity(&state.db, user.id, "register", Some("用户注册")).await {
        tracing::warn!("Failed to log register activity: {}", e);
    }

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
    let throttle_key = body.username.trim().to_lowercase();
    if is_login_rate_limited(&throttle_key) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "登录失败次数过多，请稍后再试"})),
        ));
    }

    let user = db::get_user_by_username(&state.db, &body.username)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            record_login_failure(&throttle_key);
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
        record_login_failure(&throttle_key);
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid credentials"})),
        ));
    }
    clear_login_failures(&throttle_key);

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

    if let Err(e) = db::log_activity(&state.db, user.id, "login", None).await {
        tracing::warn!("Failed to log login activity: {}", e);
    }

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

fn login_failures() -> &'static Mutex<HashMap<String, Vec<Instant>>> {
    LOGIN_FAILURES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn login_limit_window() -> Duration {
    *LOGIN_LIMIT_WINDOW_OVERRIDE.get_or_init(|| {
        let seconds = std::env::var(LOGIN_LIMIT_WINDOW_ENV)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_LOGIN_LIMIT_WINDOW_SECS);
        Duration::from_secs(seconds)
    })
}

fn login_max_failures() -> usize {
    *LOGIN_MAX_FAILURES_OVERRIDE.get_or_init(|| {
        std::env::var(LOGIN_MAX_FAILURES_ENV)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_LOGIN_MAX_FAILURES)
    })
}

fn is_login_rate_limited(key: &str) -> bool {
    let mut store = login_failures()
        .lock()
        .expect("login failure mutex poisoned");
    let attempts = store.entry(key.to_string()).or_default();
    prune_old_attempts(attempts);
    attempts.len() >= login_max_failures()
}

fn record_login_failure(key: &str) {
    let mut store = login_failures()
        .lock()
        .expect("login failure mutex poisoned");
    let attempts = store.entry(key.to_string()).or_default();
    prune_old_attempts(attempts);
    attempts.push(Instant::now());
}

fn clear_login_failures(key: &str) {
    let mut store = login_failures()
        .lock()
        .expect("login failure mutex poisoned");
    store.remove(key);
}

fn prune_old_attempts(attempts: &mut Vec<Instant>) {
    let now = Instant::now();
    attempts.retain(|at| now.duration_since(*at) < login_limit_window());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_old_attempts_keeps_recent_entries_only() {
        let now = Instant::now();
        let window = login_limit_window();
        let mut attempts = vec![
            now - (window + Duration::from_secs(1)),
            now - Duration::from_secs(1),
        ];
        prune_old_attempts(&mut attempts);
        assert_eq!(attempts.len(), 1);
    }

    #[test]
    fn login_rate_limit_triggers_after_threshold() {
        let key = format!("test-user-{}", std::process::id());
        clear_login_failures(&key);

        for _ in 0..login_max_failures() {
            record_login_failure(&key);
        }
        assert!(is_login_rate_limited(&key));

        clear_login_failures(&key);
        assert!(!is_login_rate_limited(&key));
    }
}
