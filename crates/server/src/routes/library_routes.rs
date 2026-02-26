use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

use fast_photo_core::{db, scanner};
use fast_photo_core::models::CreateLibrary;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_libraries).post(create_library))
        .route("/{id}", delete(delete_library))
        .route("/{id}/scan", post(scan_library))
        .route("/scan-progress", get(scan_progress))
}

/// List user's libraries
async fn list_libraries(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!(libs)))
}

/// Create a new library
async fn create_library(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateLibrary>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Validate path exists
    if !std::path::Path::new(&body.path).exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Path does not exist: {}", body.path)})),
        ));
    }

    let lib = db::create_library(&state.db, &body, auth.user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok((StatusCode::CREATED, Json(json!(lib))))
}

/// Delete a library
async fn delete_library(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let lib = db::get_library_by_id(&state.db, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Library not found"}))))?;

    if lib.user_id != auth.user_id && auth.role != "admin" {
        return Err((StatusCode::FORBIDDEN, Json(json!({"error": "No permission"}))));
    }

    db::delete_library(&state.db, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Trigger scan on a library
async fn scan_library(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let lib = db::get_library_by_id(&state.db, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Library not found"}))))?;

    if lib.user_id != auth.user_id && auth.role != "admin" {
        return Err((StatusCode::FORBIDDEN, Json(json!({"error": "No permission"}))));
    }

    let pool = state.db.clone();
    let scan_tx = state.scan_tx.clone();
    let library_path = lib.path.clone();
    let thumb_dir = state.config.storage.thumbnail_dir.clone();

    // Spawn scan in background
    tokio::spawn(async move {
        match scanner::scan_library(pool.clone(), id, &library_path, (*scan_tx).clone()).await {
            Ok(count) => {
                tracing::info!("Library {} scan complete: {} files", id, count);
                // Generate thumbnails after scanning
                generate_thumbnails_for_library(&pool, id, &thumb_dir).await;
            }
            Err(e) => {
                tracing::error!("Library {} scan error: {}", id, e);
                let _ = db::update_library_scan_status(&pool, id, "error", 0).await;
            }
        }
    });

    Ok(Json(json!({"status": "scanning", "message": "Scan started"})))
}

async fn generate_thumbnails_for_library(pool: &db::DbPool, library_id: i64, thumb_dir: &std::path::Path) {
    use fast_photo_core::thumbnailer;

    let result = db::get_photos_by_timeline(pool, &[library_id], 0, 100000).await;
    match result {
        Ok((photos, _)) => {
            let thumb_dir = thumb_dir.to_path_buf();
            for photo in photos {
                if !photo.has_thumbnail && photo.mime_type.starts_with("image/") {
                    let source = std::path::Path::new(&photo.file_path);
                    match thumbnailer::generate_thumbnails(source, &thumb_dir, photo.id) {
                        Ok(_) => {
                            let _ = db::set_photo_thumbnail(pool, photo.id).await;
                        }
                        Err(e) => {
                            tracing::warn!("Thumbnail error for photo {}: {}", photo.id, e);
                        }
                    }
                }
            }
            tracing::info!("Thumbnails generated for library {}", library_id);
        }
        Err(e) => {
            tracing::error!("Failed to get photos for thumbnail generation: {}", e);
        }
    }
}

/// Get current scan progress
async fn scan_progress(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Json<Value> {
    let progress = state.scan_progress.borrow().clone();
    Json(json!(progress))
}
