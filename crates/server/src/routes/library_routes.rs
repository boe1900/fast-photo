use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::path::Path as StdPath;
use std::sync::Arc;
use tokio::sync::watch;

use fast_photo_core::models::{CreateLibrary, ScanProgress};
use fast_photo_core::storage::{create_storage, StorageConfig as RemoteStorageConfig};
use fast_photo_core::{db, scanner};

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_libraries).post(create_library))
        .route("/:id", delete(delete_library))
        .route("/:id/scan", post(scan_library))
        .route("/scan-progress", get(scan_progress))
}

fn default_storage_config(state: &AppState) -> RemoteStorageConfig {
    RemoteStorageConfig::Local {
        path: state.config.storage.data_dir.to_string_lossy().to_string(),
    }
}

async fn user_storage_config(
    state: &AppState,
    user_id: i64,
) -> Result<RemoteStorageConfig, StatusCode> {
    let cfg = db::get_storage_config(&state.db, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .unwrap_or_else(|| default_storage_config(state));
    Ok(cfg)
}

fn remote_scan_root(library_path: &str, library_id: i64) -> String {
    let trimmed = library_path.trim();
    let normalized = trimmed.trim_matches('/').replace('\\', "/");
    let looks_local_abs = trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.contains(":\\")
        || trimmed.contains(":/");
    if normalized.is_empty() || looks_local_abs {
        format!("library-{}/Uploads", library_id)
    } else {
        normalized
    }
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
    let storage_cfg = user_storage_config(&state, auth.user_id)
        .await
        .map_err(|status| {
            (
                status,
                Json(json!({"error": "Failed to load storage config"})),
            )
        })?;
    if matches!(&storage_cfg, RemoteStorageConfig::Local { .. }) {
        if !std::path::Path::new(&body.path).exists() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Path does not exist: {}", body.path)})),
            ));
        }
    } else if body.path.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Remote library path (prefix) cannot be empty"})),
        ));
    }

    let lib = db::create_library(&state.db, &body, auth.user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

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
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Library not found"})),
            )
        })?;

    if lib.user_id != auth.user_id && auth.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "No permission"})),
        ));
    }

    db::delete_library(&state.db, id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

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
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Library not found"})),
            )
        })?;

    if lib.user_id != auth.user_id && auth.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "No permission"})),
        ));
    }

    let pool = state.db.clone();
    let scan_tx = state.scan_tx.clone();
    let library_path = lib.path.clone();
    let user_id = lib.user_id;
    let data_dir = state.config.storage.data_dir.clone();
    let thumb_dir = state.config.storage.thumbnail_dir.clone();
    let storage_cfg = user_storage_config(&state, lib.user_id)
        .await
        .map_err(|status| {
            (
                status,
                Json(json!({"error": "Failed to load storage config"})),
            )
        })?;

    // Spawn scan in background
    tokio::spawn(async move {
        let result = if matches!(&storage_cfg, RemoteStorageConfig::Local { .. }) {
            scanner::scan_library(pool.clone(), id, &library_path, (*scan_tx).clone()).await
        } else {
            scan_remote_library(
                pool.clone(),
                id,
                user_id,
                &library_path,
                &storage_cfg,
                (*scan_tx).clone(),
                &thumb_dir,
            )
            .await
        };
        match result {
            Ok(count) => {
                tracing::info!("Library {} scan complete: {} files", id, count);
                // Generate thumbnails after scanning
                generate_thumbnails_for_library(&pool, id, user_id, &thumb_dir, &data_dir).await;
            }
            Err(e) => {
                tracing::error!("Library {} scan error: {}", id, e);
                let _ = db::update_library_scan_status(&pool, id, "error", 0).await;
            }
        }
    });

    Ok(Json(
        json!({"status": "scanning", "message": "Scan started"}),
    ))
}

async fn scan_remote_library(
    pool: Arc<db::DbPool>,
    library_id: i64,
    user_id: i64,
    library_path: &str,
    storage_cfg: &RemoteStorageConfig,
    progress_tx: watch::Sender<ScanProgress>,
    thumb_dir: &StdPath,
) -> anyhow::Result<u64> {
    db::update_library_scan_status(&pool, library_id, "scanning", 0).await?;

    let cache_dir = thumb_dir
        .join("runtime_storage_cache")
        .join(format!("u{}", user_id))
        .join("scan-source");
    let backend = create_storage(storage_cfg, &cache_dir)?;
    let scan_root = remote_scan_root(library_path, library_id);
    let all_files = backend.list_files_recursive(&scan_root).await?;
    let media_files: Vec<String> = all_files
        .into_iter()
        .filter(|p| scanner::is_supported_media(StdPath::new(p)))
        .collect();
    let total = media_files.len() as u64;
    tracing::info!(
        "Remote scan library {} root={} media_files={}",
        library_id,
        scan_root,
        total
    );

    let _ = progress_tx.send(ScanProgress {
        library_id,
        total_files: total,
        processed_files: 0,
        status: "scanning".to_string(),
    });

    let mut processed = 0u64;
    for key in &media_files {
        match backend.get_local_path(key).await {
            Ok(local_path) => {
                if let Err(e) =
                    scanner::process_file_with_stored_path(&pool, &local_path, key, library_id)
                        .await
                {
                    tracing::warn!("Failed to process remote file {}: {}", key, e);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to cache remote file {}: {}", key, e);
            }
        }

        processed += 1;
        if processed % 50 == 0 || processed == total {
            let _ = progress_tx.send(ScanProgress {
                library_id,
                total_files: total,
                processed_files: processed,
                status: "scanning".to_string(),
            });
        }
    }

    match db::pair_live_photos(&pool, library_id).await {
        Ok(paired) => {
            if paired > 0 {
                tracing::info!("Paired {} Live Photos", paired);
            }
        }
        Err(e) => tracing::warn!("Live Photo pairing failed: {}", e),
    }

    db::update_library_scan_status(&pool, library_id, "completed", processed as i64).await?;
    let _ = progress_tx.send(ScanProgress {
        library_id,
        total_files: total,
        processed_files: processed,
        status: "completed".to_string(),
    });
    Ok(processed)
}

async fn generate_thumbnails_for_library(
    pool: &db::DbPool,
    library_id: i64,
    user_id: i64,
    thumb_dir: &std::path::Path,
    default_data_dir: &std::path::Path,
) {
    use fast_photo_core::thumbnailer;
    use std::path::PathBuf;

    let storage_cfg = db::get_storage_config(pool, user_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| RemoteStorageConfig::Local {
            path: default_data_dir.to_string_lossy().to_string(),
        });
    let mut remote_backend = if matches!(&storage_cfg, RemoteStorageConfig::Local { .. }) {
        None
    } else {
        let cache_dir = thumb_dir
            .join("runtime_storage_cache")
            .join(format!("u{}", user_id))
            .join("scan-thumb");
        match create_storage(&storage_cfg, &cache_dir) {
            Ok(backend) => Some(backend),
            Err(e) => {
                tracing::warn!(
                    library_id = library_id,
                    user_id = user_id,
                    error = %e,
                    "Failed to initialize remote storage backend for post-scan thumbnails"
                );
                None
            }
        }
    };

    let result = db::get_photos_by_timeline(pool, &[library_id], 0, 100000).await;
    match result {
        Ok((photos, _)) => {
            let thumb_dir = thumb_dir.to_path_buf();
            for photo in photos {
                if !photo.has_thumbnail && photo.mime_type.starts_with("image/") {
                    let source = if let Some(backend) = &mut remote_backend {
                        match backend.get_local_path(&photo.file_path).await {
                            Ok(path) => path,
                            Err(e) => {
                                tracing::warn!(
                                    photo_id = photo.id,
                                    file_path = %photo.file_path,
                                    error = %e,
                                    "Failed to resolve remote source for post-scan thumbnail"
                                );
                                continue;
                            }
                        }
                    } else {
                        PathBuf::from(&photo.file_path)
                    };
                    match thumbnailer::generate_thumbnails(&source, &thumb_dir, photo.id) {
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
async fn scan_progress(_auth: AuthUser, State(state): State<AppState>) -> Json<Value> {
    let progress = state.scan_progress.borrow().clone();
    Json(json!(progress))
}
