use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio_util::io::ReaderStream;

use fast_photo_core::db;
use fast_photo_core::dedup;
use fast_photo_core::models::PaginationParams;
use fast_photo_core::scanner;
use fast_photo_core::storage::{create_storage, StorageConfig as RemoteStorageConfig};
use fast_photo_core::thumbnailer::{self, ThumbnailSize};

use crate::auth::AuthUser;
use crate::state::AppState;

async fn user_library_ids(state: &AppState, user_id: i64) -> Result<Vec<i64>, StatusCode> {
    let libs = db::get_libraries(&state.db, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(libs.iter().map(|l| l.id).collect())
}

async fn ensure_photo_access(
    state: &AppState,
    user_id: i64,
    photo_id: i64,
) -> Result<(), StatusCode> {
    let lib_ids = user_library_ids(state, user_id).await?;
    let allowed = db::photo_in_libraries(&state.db, photo_id, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
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

fn storage_backend_for_user(
    state: &AppState,
    user_id: i64,
    cfg: &RemoteStorageConfig,
    scope: &str,
) -> Result<Box<dyn fast_photo_core::storage::StorageBackend>, StatusCode> {
    let cache_dir = state
        .config
        .storage
        .thumbnail_dir
        .join("runtime_storage_cache")
        .join(format!("u{}", user_id))
        .join(scope);
    create_storage(cfg, &cache_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn next_upload_name(original_file_name: &str, counter: u32) -> String {
    let path = std::path::Path::new(original_file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("upload");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.is_empty() {
        format!("{} ({})", stem, counter)
    } else {
        format!("{} ({}).{}", stem, counter, ext)
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/timeline", get(timeline))
        .route("/folders", get(folders))
        .route("/folder-contents", get(folder_contents))
        .route("/folders/contents", get(folder_contents))
        .route("/search", get(search))
        .route("/geo", get(geo_photos))
        .route("/favorites", get(favorites))
        .route("/trash", get(trash_list))
        .route("/trash/empty", post(empty_trash))
        .route("/duplicates", get(duplicates))
        .route("/upload", post(upload_photo))
        .route("/batch/favorite", post(batch_favorite))
        .route("/batch/trash", post(batch_trash))
        .route("/batch/restore", post(batch_restore))
        .route("/:id", get(photo_detail))
        .route("/:id/thumbnail/:size", get(thumbnail))
        .route("/:id/original", get(original))
        .route("/:id/live-video", get(live_video))
        .route("/:id/favorite", post(toggle_favorite))
        .route("/:id/trash", post(trash_photo))
        .route("/:id/restore", post(restore_photo))
        .route("/:id/permanent", delete(permanent_delete))
}

#[derive(Debug, Deserialize)]
struct TimelineQuery {
    #[serde(flatten)]
    pagination: PaginationParams,
}

/// Get photos in timeline order
async fn timeline(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;

    let (photos, total) = db::get_photos_by_timeline(
        &state.db,
        &lib_ids,
        query.pagination.offset(),
        query.pagination.per_page(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "data": photos,
        "total": total,
        "page": query.pagination.page.unwrap_or(1),
        "per_page": query.pagination.per_page(),
    })))
}

/// Get folder structure
async fn folders(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;

    let folder_tree = db::get_folder_tree(&state.db, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!(folder_tree)))
}

#[derive(Debug, Deserialize)]
struct FolderQuery {
    path: String,
    #[serde(flatten)]
    pagination: PaginationParams,
}

/// Get photos in a specific folder
async fn folder_contents(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<FolderQuery>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;
    let (photos, total) = db::get_photos_by_folder(
        &state.db,
        &lib_ids,
        &query.path,
        query.pagination.offset(),
        query.pagination.per_page(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "data": photos,
        "total": total,
        "page": query.pagination.page.unwrap_or(1),
        "per_page": query.pagination.per_page(),
    })))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(flatten)]
    pagination: PaginationParams,
}

/// Search photos
async fn search(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;

    let (photos, total) = db::search_photos(
        &state.db,
        &query.q,
        &lib_ids,
        query.pagination.offset(),
        query.pagination.per_page(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "data": photos,
        "total": total,
        "page": query.pagination.page.unwrap_or(1),
        "per_page": query.pagination.per_page(),
    })))
}

/// Get photo detail
async fn photo_detail(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    ensure_photo_access(&state, auth.user_id, id).await?;
    let photo = db::get_photo_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!({
        "photo": photo,
        "thumbnail_url": format!("/api/photos/{}/thumbnail/small", id),
        "medium_url": format!("/api/photos/{}/thumbnail/medium", id),
        "full_url": format!("/api/photos/{}/original", id),
    })))
}

/// Serve thumbnail
async fn thumbnail(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, size)): Path<(i64, String)>,
) -> Result<(StatusCode, [(header::HeaderName, String); 2], Body), StatusCode> {
    ensure_photo_access(&state, auth.user_id, id).await?;
    let thumb_size = match size.as_str() {
        "small" => ThumbnailSize::Small,
        "medium" => ThumbnailSize::Medium,
        "large" => ThumbnailSize::Large,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let thumb_path =
        thumbnailer::thumbnail_path(&state.config.storage.thumbnail_dir, id, thumb_size);

    if !thumb_path.exists() {
        // Try to generate on-the-fly
        let photo = db::get_photo_by_id(&state.db, id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

        let storage_cfg = user_storage_config(&state, auth.user_id).await?;
        let mut source = PathBuf::from(&photo.file_path);
        let mut temp_source: Option<PathBuf> = None;
        if !matches!(&storage_cfg, RemoteStorageConfig::Local { .. }) {
            let backend = storage_backend_for_user(&state, auth.user_id, &storage_cfg, "thumb")?;
            match backend.get_local_path(&photo.file_path).await {
                Ok(remote_local_path) => {
                    source = remote_local_path;
                }
                Err(cache_err) => {
                    tracing::warn!(
                        photo_id = id,
                        file_path = %photo.file_path,
                        error = %cache_err,
                        "Failed to cache remote file for thumbnail, falling back to temp download"
                    );
                    let data = backend.read_file(&photo.file_path).await.map_err(|err| {
                        tracing::error!(
                            photo_id = id,
                            file_path = %photo.file_path,
                            error = %err,
                            "Failed to read remote file for thumbnail"
                        );
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                    let ext = std::path::Path::new(&photo.file_path)
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("bin");
                    let tmp_path = std::env::temp_dir().join(format!(
                        "fast-photo-thumb-src-{}-{}.{}",
                        id,
                        uuid::Uuid::new_v4(),
                        ext
                    ));
                    tokio::fs::write(&tmp_path, &data).await.map_err(|err| {
                        tracing::error!(
                            photo_id = id,
                            path = %tmp_path.to_string_lossy(),
                            error = %err,
                            "Failed to write temporary thumbnail source"
                        );
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                    source = tmp_path.clone();
                    temp_source = Some(tmp_path);
                }
            }
        }

        let generate_result = if photo.mime_type.starts_with("video/") {
            thumbnailer::generate_video_thumbnail(&source, &state.config.storage.thumbnail_dir, id)
        } else {
            thumbnailer::generate_thumbnail(
                &source,
                &state.config.storage.thumbnail_dir,
                id,
                thumb_size,
            )
            .map(|_| ())
        };

        if let Some(tmp_path) = temp_source {
            let _ = tokio::fs::remove_file(tmp_path).await;
        }

        generate_result.map_err(|err| {
            tracing::error!(
                photo_id = id,
                source = %source.to_string_lossy(),
                mime_type = %photo.mime_type,
                error = %err,
                "Failed to generate thumbnail"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    let file = tokio::fs::File::open(&thumb_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/webp".to_string()),
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000".to_string(),
            ),
        ],
        body,
    ))
}

/// Serve original photo
async fn original(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, [(header::HeaderName, String); 2], Body), StatusCode> {
    ensure_photo_access(&state, auth.user_id, id).await?;
    let photo = db::get_photo_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let storage_cfg = user_storage_config(&state, auth.user_id).await?;
    if !matches!(&storage_cfg, RemoteStorageConfig::Local { .. }) {
        let backend = storage_backend_for_user(&state, auth.user_id, &storage_cfg, "original")?;
        if let Ok(data) = backend.read_file(&photo.file_path).await {
            return Ok((
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, photo.mime_type.clone()),
                    (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
                ],
                Body::from(data),
            ));
        }
    }

    let file = tokio::fs::File::open(&photo.file_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let body = Body::from_stream(ReaderStream::new(file));

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, photo.mime_type.clone()),
            (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
        ],
        body,
    ))
}

/// Get all geo-tagged photos for map view
async fn geo_photos(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;

    let photos = db::get_geo_photos(&state.db, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!(photos)))
}

/// Serve Live Photo video
async fn live_video(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, [(header::HeaderName, String); 3], Body), StatusCode> {
    ensure_photo_access(&state, auth.user_id, id).await?;
    let photo = db::get_photo_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let video_path = photo.live_photo_video_path.ok_or(StatusCode::NOT_FOUND)?;

    let file = tokio::fs::File::open(&video_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let metadata = file
        .metadata()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "video/quicktime".to_string()),
            (header::CONTENT_LENGTH, metadata.len().to_string()),
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000".to_string(),
            ),
        ],
        body,
    ))
}

// ─── Favorites ─────────────────────────────────

async fn toggle_favorite(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    ensure_photo_access(&state, auth.user_id, id).await?;
    let is_fav = db::toggle_favorite(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "is_favorite": is_fav })))
}

async fn favorites(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(50);
    let offset = ((page - 1) * per_page) as i64;

    let (photos, total) = db::get_favorites(&state.db, &lib_ids, per_page as i64, offset)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "data": photos,
        "total": total,
        "page": page,
        "per_page": per_page,
    })))
}

// ─── Trash ─────────────────────────────────────

async fn trash_photo(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    ensure_photo_access(&state, auth.user_id, id).await?;
    db::trash_photo(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn restore_photo(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    ensure_photo_access(&state, auth.user_id, id).await?;
    db::restore_photo(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn permanent_delete(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    ensure_photo_access(&state, auth.user_id, id).await?;
    let storage_cfg = user_storage_config(&state, auth.user_id).await?;
    let remote_backend = if matches!(&storage_cfg, RemoteStorageConfig::Local { .. }) {
        None
    } else {
        Some(storage_backend_for_user(
            &state,
            auth.user_id,
            &storage_cfg,
            "delete",
        )?)
    };
    let file_path = db::permanent_delete(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(backend) = remote_backend {
        let _ = backend.delete_file(&file_path).await;
    } else {
        let _ = tokio::fs::remove_file(&file_path).await;
    }
    Ok(StatusCode::OK)
}

async fn trash_list(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(50);
    let offset = ((page - 1) * per_page) as i64;

    let (photos, total) = db::get_trash(&state.db, &lib_ids, per_page as i64, offset)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "data": photos,
        "total": total,
        "page": page,
        "per_page": per_page,
    })))
}

async fn empty_trash(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;
    let storage_cfg = user_storage_config(&state, auth.user_id).await?;
    let remote_backend = if matches!(&storage_cfg, RemoteStorageConfig::Local { .. }) {
        None
    } else {
        Some(storage_backend_for_user(
            &state,
            auth.user_id,
            &storage_cfg,
            "empty-trash",
        )?)
    };

    let paths = db::empty_trash(&state.db, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Best-effort remove of underlying files after DB rows are deleted.
    for path in &paths {
        if let Some(backend) = &remote_backend {
            let _ = backend.delete_file(path).await;
        } else {
            let _ = tokio::fs::remove_file(path).await;
        }
    }

    let count = paths.len();
    Ok(Json(json!({ "deleted": count })))
}

// ─── Batch operations ──────────────────────────

#[derive(Deserialize)]
struct BatchRequest {
    photo_ids: Vec<i64>,
}

#[derive(Deserialize)]
struct BatchFavoriteRequest {
    photo_ids: Vec<i64>,
    favorite: bool,
}

async fn batch_favorite(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<BatchFavoriteRequest>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;
    let allowed_ids = db::filter_photo_ids_in_libraries(&state.db, &body.photo_ids, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let affected = db::set_favorite_batch(&state.db, &allowed_ids, body.favorite)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "affected": affected })))
}

async fn batch_trash(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<BatchRequest>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;
    let allowed_ids = db::filter_photo_ids_in_libraries(&state.db, &body.photo_ids, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let affected = db::trash_batch(&state.db, &allowed_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "affected": affected })))
}

async fn batch_restore(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<BatchRequest>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;
    let allowed_ids = db::filter_photo_ids_in_libraries(&state.db, &body.photo_ids, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let affected = db::restore_batch(&state.db, &allowed_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "affected": affected })))
}

// ─── Upload ────────────────────────────────────

async fn upload_photo(
    auth: AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Value>, StatusCode> {
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if libs.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let library = &libs[0]; // Use first library as upload target
    let storage_cfg = user_storage_config(&state, auth.user_id).await?;
    let mut remote_backend = if matches!(&storage_cfg, RemoteStorageConfig::Local { .. }) {
        None
    } else {
        Some(storage_backend_for_user(
            &state,
            auth.user_id,
            &storage_cfg,
            "upload",
        )?)
    };
    let upload_dir = std::path::Path::new(&library.path).join("Uploads");
    if remote_backend.is_none() {
        tokio::fs::create_dir_all(&upload_dir)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let mut uploaded = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let original_file_name = field.file_name().unwrap_or("unknown").to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

        let mut file_name = original_file_name.clone();
        let stored_path: String;
        let analysis_source: PathBuf;
        let mut cleanup_temp: Option<PathBuf> = None;

        if let Some(backend) = remote_backend.as_mut() {
            let upload_prefix = format!("library-{}/Uploads", library.id);
            let mut counter = 1u32;
            let mut remote_path = format!("{}/{}", upload_prefix, file_name);
            while backend
                .exists(&remote_path)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            {
                file_name = next_upload_name(&original_file_name, counter);
                remote_path = format!("{}/{}", upload_prefix, file_name);
                counter += 1;
            }

            backend
                .write_file(&remote_path, &data)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            stored_path = remote_path;

            let tmp_path = std::env::temp_dir().join(format!(
                "fast-photo-upload-{}-{}",
                auth.user_id,
                uuid::Uuid::new_v4()
            ));
            tokio::fs::write(&tmp_path, &data)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            analysis_source = tmp_path.clone();
            cleanup_temp = Some(tmp_path);
        } else {
            // Keep existing files and choose an available filename suffix.
            let mut dest_path = upload_dir.join(&file_name);
            let mut counter = 1u32;
            while tokio::fs::try_exists(&dest_path)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            {
                file_name = next_upload_name(&original_file_name, counter);
                dest_path = upload_dir.join(&file_name);
                counter += 1;
            }

            tokio::fs::write(&dest_path, &data)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            stored_path = dest_path.to_string_lossy().to_string();
            analysis_source = dest_path;
        }

        let file_size = data.len() as i64;

        // Calculate file hash for dedup
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(&data));

        let photo_id = db::insert_uploaded_photo(
            &state.db,
            &stored_path,
            &file_name,
            file_size,
            &content_type,
            library.id,
            Some(&hash),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let guessed_mime = scanner::mime_type_from_extension(std::path::Path::new(&file_name));
        if content_type.starts_with("image/") || guessed_mime.starts_with("image/") {
            let phash = dedup::compute_dhash(&analysis_source).unwrap_or_else(|_| hash.clone());
            let _ = db::update_phash(&state.db, photo_id, &phash).await;
        }
        if let Some(tmp_path) = cleanup_temp {
            let _ = tokio::fs::remove_file(tmp_path).await;
        }

        uploaded.push(json!({ "id": photo_id, "file_name": file_name }));
    }

    Ok(Json(json!({ "uploaded": uploaded })))
}

// ─── Duplicates ────────────────────────────────

async fn duplicates(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let lib_ids = user_library_ids(&state, auth.user_id).await?;

    let groups = db::find_duplicates(&state.db, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result: Vec<Value> = groups
        .into_iter()
        .map(|(hash, photos)| {
            json!({
                "hash": hash,
                "count": photos.len(),
                "photos": photos,
            })
        })
        .collect();

    Ok(Json(
        json!({ "groups": result, "total_groups": result.len() }),
    ))
}
