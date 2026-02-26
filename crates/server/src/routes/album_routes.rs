use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::state::AppState;
use axum::http::HeaderMap;
use fast_photo_core::db;
use fast_photo_core::models::PaginationParams;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_albums).post(create_album))
        .route("/:id", get(get_album).delete(delete_album_handler))
        .route("/:id/photos", get(album_photos).post(add_to_album))
        .route("/:id/photos/:photo_id", delete(remove_from_album))
        .route("/:id/share-password", post(set_share_password))
}

pub fn share_routes() -> Router<AppState> {
    Router::new()
        .route("/:token", get(shared_album))
        .route("/:token/photos", get(shared_album_photos))
        .route("/:token/verify", post(verify_share_password))
}

async fn ensure_album_owner(
    state: &AppState,
    auth: &AuthUser,
    album_id: i64,
) -> Result<fast_photo_core::models::Album, StatusCode> {
    let album = db::get_album_by_id(&state.db, album_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if album.user_id != auth.user_id && auth.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(album)
}

async fn list_albums(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let albums = db::get_albums(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!(albums)))
}

#[derive(Deserialize)]
struct CreateAlbumReq {
    name: String,
}

async fn create_album(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateAlbumReq>,
) -> Result<Json<Value>, StatusCode> {
    let album = db::create_album(&state.db, &body.name, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!(album)))
}

async fn get_album(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    let album = ensure_album_owner(&state, &auth, id).await?;
    Ok(Json(json!(album)))
}

async fn delete_album_handler(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    ensure_album_owner(&state, &auth, id).await?;
    db::delete_album(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn album_photos(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<Value>, StatusCode> {
    ensure_album_owner(&state, &auth, id).await?;
    let (photos, total) =
        db::get_album_photos(&state.db, id, pagination.offset(), pagination.per_page())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({
        "data": photos,
        "total": total,
    })))
}

#[derive(Deserialize)]
struct AddPhotosReq {
    photo_ids: Vec<i64>,
}

async fn add_to_album(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AddPhotosReq>,
) -> Result<StatusCode, StatusCode> {
    ensure_album_owner(&state, &auth, id).await?;

    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();
    let allowed_ids = db::filter_photo_ids_in_libraries(&state.db, &body.photo_ids, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if allowed_ids.len() != body.photo_ids.len() {
        return Err(StatusCode::FORBIDDEN);
    }

    db::add_photos_to_album(&state.db, id, &allowed_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn remove_from_album(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((album_id, photo_id)): Path<(i64, i64)>,
) -> Result<StatusCode, StatusCode> {
    ensure_album_owner(&state, &auth, album_id).await?;
    db::remove_photo_from_album(&state.db, album_id, photo_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct SharePasswordReq {
    password: Option<String>,
}

async fn set_share_password(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SharePasswordReq>,
) -> Result<Json<Value>, StatusCode> {
    ensure_album_owner(&state, &auth, id).await?;
    db::set_share_password(&state.db, id, body.password.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "ok": true })))
}

// ─── Public share routes (no auth) ─────────────────────

async fn shared_album(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let album = db::get_album_by_share_token(&state.db, &token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let has_password = album.share_password.is_some();
    Ok(Json(json!({
        "id": album.id,
        "name": album.name,
        "photo_count": album.photo_count,
        "has_password": has_password,
    })))
}

#[derive(Deserialize)]
struct VerifyPasswordReq {
    password: String,
}

async fn verify_share_password(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<VerifyPasswordReq>,
) -> Result<Json<Value>, StatusCode> {
    let album = db::get_album_by_share_token(&state.db, &token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let valid = match &album.share_password {
        Some(pw) => pw == &body.password,
        None => true,
    };
    if valid {
        Ok(Json(json!({ "valid": true })))
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn shared_album_photos(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<Value>, StatusCode> {
    let album = db::get_album_by_share_token(&state.db, &token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Check password if set
    if let Some(pw) = &album.share_password {
        let provided = headers
            .get("x-share-password")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != pw {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    let (photos, total) = db::get_album_photos(
        &state.db,
        album.id,
        pagination.offset(),
        pagination.per_page(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({
        "data": photos,
        "total": total,
        "album_name": album.name,
    })))
}
