use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use fast_photo_core::db;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/tags", get(list_tags))
        .route("/tags/photos", get(photos_by_tag))
        .route("/photos/:id/tags", get(photo_tags))
        .route("/photos/:id/tags", post(add_tag))
        .route("/photos/:id/tags/:tag_id", delete(remove_tag))
}

async fn ensure_photo_access(
    state: &AppState,
    auth: &AuthUser,
    photo_id: i64,
) -> Result<(), StatusCode> {
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();
    let allowed = db::photo_in_libraries(&state.db, photo_id, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

async fn list_tags(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let tags = db::list_all_tags(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "tags": tags })))
}

async fn photo_tags(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    ensure_photo_access(&state, &auth, id).await?;
    let tags = db::get_photo_tags(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "tags": tags })))
}

#[derive(Deserialize)]
struct AddTagRequest {
    name: String,
    #[serde(default = "default_category")]
    category: String,
}

fn default_category() -> String {
    "manual".to_string()
}

async fn add_tag(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AddTagRequest>,
) -> Result<Json<Value>, StatusCode> {
    ensure_photo_access(&state, &auth, id).await?;
    let tag_id = db::get_or_create_tag(&state.db, &body.name, &body.category)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db::add_tag_to_photo(&state.db, id, tag_id, 1.0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = db::log_activity(
        &state.db,
        auth.user_id,
        "add_tag",
        Some(&format!("photo:{} tag:{}", id, body.name)),
    )
    .await;
    Ok(Json(json!({ "tag_id": tag_id })))
}

async fn remove_tag(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, tag_id)): Path<(i64, i64)>,
) -> Result<StatusCode, StatusCode> {
    ensure_photo_access(&state, &auth, id).await?;
    db::remove_tag_from_photo(&state.db, id, tag_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = db::log_activity(
        &state.db,
        auth.user_id,
        "remove_tag",
        Some(&format!("photo:{} tag_id:{}", id, tag_id)),
    )
    .await;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct TagSearchParams {
    tag: String,
    #[serde(
        default,
        deserialize_with = "fast_photo_core::models::deserialize_opt_u32_from_string"
    )]
    page: Option<u32>,
    #[serde(
        default,
        deserialize_with = "fast_photo_core::models::deserialize_opt_u32_from_string"
    )]
    per_page: Option<u32>,
}

async fn photos_by_tag(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<TagSearchParams>,
) -> Result<Json<Value>, StatusCode> {
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(50);
    let offset = ((page - 1) * per_page) as i64;

    let (photos, total) =
        db::search_photos_by_tag(&state.db, &params.tag, &lib_ids, per_page as i64, offset)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "data": photos,
        "total": total,
        "page": page,
        "per_page": per_page,
    })))
}
