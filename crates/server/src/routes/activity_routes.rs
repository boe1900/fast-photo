use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

use fast_photo_core::db;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/activity", get(activity_logs))
}

async fn activity_logs(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let logs = db::get_activity_logs(&state.db, auth.user_id, 100)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "logs": logs })))
}
