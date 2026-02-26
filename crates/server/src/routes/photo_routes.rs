use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::io::ReaderStream;

use fast_photo_core::db;
use fast_photo_core::models::PaginationParams;
use fast_photo_core::thumbnailer::{self, ThumbnailSize};

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/timeline", get(timeline))
        .route("/folders", get(folders))
        .route("/folder-contents", get(folder_contents))
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
        .route("/{id}", get(photo_detail))
        .route("/{id}/thumbnail/{size}", get(thumbnail))
        .route("/{id}/original", get(original))
        .route("/{id}/live-video", get(live_video))
        .route("/{id}/favorite", post(toggle_favorite))
        .route("/{id}/trash", post(trash_photo))
        .route("/{id}/restore", post(restore_photo))
        .route("/{id}/permanent", delete(permanent_delete))
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
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

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
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

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
    _auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<FolderQuery>,
) -> Result<Json<Value>, StatusCode> {
    let (photos, total) = db::get_photos_by_folder(
        &state.db,
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
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

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
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
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
    _auth: AuthUser,
    State(state): State<AppState>,
    Path((id, size)): Path<(i64, String)>,
) -> Result<(StatusCode, [(header::HeaderName, String); 2], Body), StatusCode> {
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

        let source = std::path::Path::new(&photo.file_path);
        if photo.mime_type.starts_with("video/") {
            thumbnailer::generate_video_thumbnail(source, &state.config.storage.thumbnail_dir, id)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        } else {
            thumbnailer::generate_thumbnail(
                source,
                &state.config.storage.thumbnail_dir,
                id,
                thumb_size,
            )
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
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
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, [(header::HeaderName, String); 2], Body), StatusCode> {
    let photo = db::get_photo_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let file = tokio::fs::File::open(&photo.file_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

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
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

    let photos = db::get_geo_photos(&state.db, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!(photos)))
}

/// Serve Live Photo video
async fn live_video(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, [(header::HeaderName, String); 3], Body), StatusCode> {
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
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
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
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();
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
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    db::trash_photo(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn restore_photo(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    db::restore_photo(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn permanent_delete(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let file_path = db::permanent_delete(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Optionally delete physical file
    let _ = tokio::fs::remove_file(&file_path).await;
    Ok(StatusCode::OK)
}

async fn trash_list(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, StatusCode> {
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();
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
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

    let paths = db::empty_trash(&state.db, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
    _auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<BatchFavoriteRequest>,
) -> Result<Json<Value>, StatusCode> {
    let affected = db::set_favorite_batch(&state.db, &body.photo_ids, body.favorite)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "affected": affected })))
}

async fn batch_trash(
    _auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<BatchRequest>,
) -> Result<Json<Value>, StatusCode> {
    let affected = db::trash_batch(&state.db, &body.photo_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "affected": affected })))
}

async fn batch_restore(
    _auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<BatchRequest>,
) -> Result<Json<Value>, StatusCode> {
    let affected = db::restore_batch(&state.db, &body.photo_ids)
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
    let upload_dir = std::path::Path::new(&library.path).join("Uploads");
    tokio::fs::create_dir_all(&upload_dir)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut uploaded = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let file_name = field.file_name().unwrap_or("unknown").to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

        let dest_path = upload_dir.join(&file_name);
        tokio::fs::write(&dest_path, &data)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let file_size = data.len() as i64;
        let path_str = dest_path.to_string_lossy().to_string();

        // Calculate file hash for dedup
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(&data));

        let photo_id = db::insert_uploaded_photo(
            &state.db,
            &path_str,
            &file_name,
            file_size,
            &content_type,
            library.id,
            Some(&hash),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        uploaded.push(json!({ "id": photo_id, "file_name": file_name }));
    }

    Ok(Json(json!({ "uploaded": uploaded })))
}

// ─── Duplicates ────────────────────────────────

async fn duplicates(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

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
