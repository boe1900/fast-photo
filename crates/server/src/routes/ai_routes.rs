use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use fast_photo_ai::clip;
use fast_photo_core::db;
use fast_photo_core::models::PaginationParams;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/tags", get(list_tags))
        .route("/tags/:id/photos", get(photos_by_tag))
        .route("/semantic-search", get(semantic_search))
        .route("/process", post(process_ai))
}

/// List all tags with photo counts
async fn list_tags(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

    let tags = db::get_tags_with_counts_in_libraries(&state.db, &lib_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Add Chinese labels
    let tags_with_labels: Vec<Value> = tags
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "name_zh": clip::scene_label_zh(&t.name),
                "category": t.category,
                "photo_count": t.photo_count,
            })
        })
        .collect();

    Ok(Json(json!(tags_with_labels)))
}

/// Get photos by tag
async fn photos_by_tag(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<Value>, StatusCode> {
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

    let (photos, total) = db::get_photos_by_tag_in_libraries(
        &state.db,
        tag_id,
        &lib_ids,
        pagination.offset(),
        pagination.per_page(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "data": photos,
        "total": total,
        "page": pagination.page.unwrap_or(1),
        "per_page": pagination.per_page(),
    })))
}

#[derive(Debug, Deserialize)]
struct SemanticSearchQuery {
    q: String,
    #[serde(
        default,
        deserialize_with = "fast_photo_core::models::deserialize_opt_u32_from_string"
    )]
    limit: Option<u32>,
}

/// Semantic search using CLIP text-to-image similarity
async fn semantic_search(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SemanticSearchQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let ai = state.ai.clip.read().await;
    let clip_model = ai.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "CLIP model not loaded. Run models/download.sh first."})),
        )
    })?;

    // Encode the query text
    let text_embedding = clip_model.encode_text(&query.q).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Text encoding error: {}", e)})),
        )
    })?;

    // Get user's library IDs
    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

    // Get all embeddings from the database
    let embeddings = db::get_all_embeddings(&state.db, &lib_ids)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    // Calculate similarities and rank
    let mut scored: Vec<(i64, f32)> = embeddings
        .iter()
        .map(|(id, bytes)| {
            let img_emb = db::decode_embedding(bytes);
            let score = fast_photo_ai::processor::cosine_similarity(&img_emb, &text_embedding);
            (*id, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let limit = query.limit.unwrap_or(50).min(200) as usize;
    let top_results: Vec<(i64, f32)> = scored.into_iter().take(limit).collect();

    // Fetch photo details for top results
    let mut photos_with_scores: Vec<Value> = Vec::new();
    for (photo_id, score) in &top_results {
        if let Ok(Some(photo)) = db::get_photo_by_id(&state.db, *photo_id).await {
            photos_with_scores.push(json!({
                "photo": photo,
                "score": score,
            }));
        }
    }

    Ok(Json(json!({
        "data": photos_with_scores,
        "query": query.q,
        "total": photos_with_scores.len(),
    })))
}

/// Trigger AI processing for unprocessed photos
async fn process_ai(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let ai_state = state.ai.clone();
    let has_clip = ai_state.clip.read().await.is_some();

    if !has_clip {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "CLIP model not loaded. Run models/download.sh first."})),
        ));
    }

    let libs = db::get_libraries(&state.db, auth.user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    let lib_ids: Vec<i64> = libs.iter().map(|l| l.id).collect();

    let pool = state.db.clone();

    // Spawn AI processing in background
    tokio::spawn(async move {
        let photos = match db::get_photos_without_embeddings(&pool, &lib_ids, 10000).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to get unprocessed photos: {}", e);
                return;
            }
        };

        tracing::info!("AI processing {} photos...", photos.len());

        let clip_guard = ai_state.clip.read().await;
        let clip_model = match clip_guard.as_ref() {
            Some(m) => m,
            None => return,
        };

        for (i, photo) in photos.iter().enumerate() {
            if !photo.mime_type.starts_with("image/") {
                continue;
            }

            let path = std::path::Path::new(&photo.file_path);
            if !path.exists() {
                continue;
            }

            // Load image
            let img = match image::open(path) {
                Ok(img) => img,
                Err(e) => {
                    tracing::warn!("Failed to open {:?}: {}", path, e);
                    continue;
                }
            };

            // Generate CLIP embedding
            match clip_model.encode_image(&img) {
                Ok(embedding) => {
                    // Save embedding
                    if let Err(e) = db::save_clip_embedding(&pool, photo.id, &embedding).await {
                        tracing::warn!("Failed to save embedding for photo {}: {}", photo.id, e);
                        continue;
                    }

                    // Classify scene using zero-shot
                    match clip_model.classify(&embedding, clip::SCENE_LABELS) {
                        Ok(results) => {
                            // Save top 3 tags with confidence > 0.2
                            for (label, score) in results.iter().take(3) {
                                if *score > 0.2 {
                                    match db::upsert_tag(&pool, label, "scene").await {
                                        Ok(tag_id) => {
                                            let _ =
                                                db::add_photo_tag(&pool, photo.id, tag_id, *score)
                                                    .await;
                                        }
                                        Err(e) => {
                                            tracing::warn!("Tag upsert error: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Classification error for photo {}: {}", photo.id, e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("CLIP encoding error for photo {}: {}", photo.id, e);
                }
            }

            if (i + 1) % 10 == 0 {
                tracing::info!("AI processed {}/{}", i + 1, photos.len());
            }
        }

        tracing::info!("✅ AI processing complete for {} photos", photos.len());
    });

    Ok(Json(
        json!({"status": "processing", "message": "AI processing started in background"}),
    ))
}
