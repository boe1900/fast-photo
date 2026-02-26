use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use fast_photo_core::db;
use fast_photo_core::storage::{create_storage, StorageConfig as RemoteStorageConfig};

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/storage", get(get_storage_config).put(save_storage_config))
        .route("/storage/test", post(test_storage_config))
}

async fn get_storage_config(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<RemoteStorageConfig>, (StatusCode, Json<Value>)> {
    let cfg = db::get_storage_config(&state.db, auth.user_id)
        .await
        .map_err(internal_error)?
        .unwrap_or_else(|| default_storage_config(&state));
    Ok(Json(cfg))
}

async fn save_storage_config(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(config): Json<RemoteStorageConfig>,
) -> Result<Json<RemoteStorageConfig>, (StatusCode, Json<Value>)> {
    validate_storage_config(&config)?;

    db::upsert_storage_config(&state.db, auth.user_id, &config)
        .await
        .map_err(internal_error)?;

    Ok(Json(config))
}

async fn test_storage_config(
    _auth: AuthUser,
    State(state): State<AppState>,
    Json(config): Json<RemoteStorageConfig>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    validate_storage_config(&config)?;

    match &config {
        RemoteStorageConfig::Local { path } => {
            let metadata = tokio::fs::metadata(path).await.map_err(|e| {
                bad_request_error(format!("本地路径不可访问: {}", e))
            })?;
            if !metadata.is_dir() {
                return Err(bad_request_error("本地路径必须是目录"));
            }
            let _ = tokio::fs::read_dir(path).await.map_err(|e| {
                bad_request_error(format!("本地路径不可读取: {}", e))
            })?;
            Ok(Json(json!({"ok": true, "message": "本地存储连接成功"})))
        }
        _ => {
            let cache_dir = state.config.storage.thumbnail_dir.join("storage-test-cache");
            let backend = create_storage(&config, &cache_dir).map_err(|e| {
                bad_request_error(format!("创建存储客户端失败: {}", e))
            })?;

            let probe = tokio::time::timeout(Duration::from_secs(10), backend.list_files("")).await;
            match probe {
                Ok(Ok(_)) => Ok(Json(json!({"ok": true, "message": "连接成功"}))),
                Ok(Err(e)) => Err(bad_request_error(format!("连接失败: {}", e))),
                Err(_) => Err(bad_request_error("连接超时，请检查网络和配置")),
            }
        }
    }
}

fn validate_storage_config(config: &RemoteStorageConfig) -> Result<(), (StatusCode, Json<Value>)> {
    match config {
        RemoteStorageConfig::Local { path } => {
            if path.trim().is_empty() {
                return Err(bad_request_error("本地存储路径不能为空"));
            }
        }
        RemoteStorageConfig::S3 {
            bucket,
            region,
            access_key,
            secret_key,
            ..
        } => {
            if bucket.trim().is_empty()
                || region.trim().is_empty()
                || access_key.trim().is_empty()
                || secret_key.trim().is_empty()
            {
                return Err(bad_request_error("S3 配置不完整"));
            }
        }
        RemoteStorageConfig::WebDav {
            url,
            username,
            password,
            ..
        } => {
            if url.trim().is_empty() || username.trim().is_empty() || password.trim().is_empty() {
                return Err(bad_request_error("WebDAV 配置不完整"));
            }
        }
    }
    Ok(())
}

fn default_storage_config(state: &AppState) -> RemoteStorageConfig {
    RemoteStorageConfig::Local {
        path: state.config.storage.data_dir.to_string_lossy().to_string(),
    }
}

fn bad_request_error(msg: impl Into<String>) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg.into() })))
}

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": err.to_string() })),
    )
}
