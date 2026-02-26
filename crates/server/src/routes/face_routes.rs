use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    routing::{get, post, put},
    Json, Router,
};
use fast_photo_core::db;
use serde::{Deserialize, Serialize};
use sqlx;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/persons", get(list_persons))
        .route("/persons/{id}", put(rename_person))
        .route("/persons/{id}/photos", get(person_photos))
        .route("/faces/scan", post(scan_faces))
        .route("/faces/cluster", post(cluster_faces))
        .route("/faces/{id}/thumbnail", get(face_thumbnail))
}

#[derive(Serialize)]
struct PersonResponse {
    id: i64,
    name: Option<String>,
    face_count: i64,
    cover_face_id: Option<i64>,
}

async fn list_persons(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PersonResponse>>, StatusCode> {
    let persons = db::list_persons(&state.db, auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result: Vec<PersonResponse> = persons
        .into_iter()
        .map(|p| PersonResponse {
            id: p.id,
            name: p.name,
            face_count: p.face_count,
            cover_face_id: p.cover_face_id,
        })
        .collect();

    Ok(Json(result))
}

#[derive(Deserialize)]
struct RenameRequest {
    name: String,
}

async fn rename_person(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<RenameRequest>,
) -> Result<StatusCode, StatusCode> {
    db::rename_person(&state.db, id, &body.name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn person_photos(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (photos, total) = db::get_person_photos(&state.db, id, 200, 0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "data": photos,
        "total": total,
    })))
}

async fn face_thumbnail(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, [(header::HeaderName, String); 2], Vec<u8>), StatusCode> {
    let data = db::get_face_thumbnail(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg".to_string()),
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000".to_string(),
            ),
        ],
        data,
    ))
}

/// Scan all photos for faces (background task)
async fn scan_faces(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let ai = state.ai.face.read().await;
    let face_model = match ai.as_ref() {
        Some(m) => m,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    // Get all photos that haven't been face-processed
    let photos = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, file_path, mime_type FROM photos WHERE mime_type LIKE 'image/%'",
    )
    .fetch_all(&*state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut processed = 0u64;
    let mut faces_found = 0u64;

    for (photo_id, file_path, _mime) in &photos {
        // Skip if already processed
        if db::has_faces_processed(&state.db, *photo_id)
            .await
            .unwrap_or(true)
        {
            continue;
        }

        let img = match image::open(file_path) {
            Ok(img) => img,
            Err(_) => continue,
        };

        let detections = match face_model.detect_faces(&img) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Face detection failed for {}: {}", file_path, e);
                continue;
            }
        };

        for det in &detections {
            let face_crop = fast_photo_ai::face::FaceModel::crop_face(&img, det, 0.2);

            // Generate embedding
            let embedding = match face_model.extract_embedding(&face_crop) {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Generate small JPEG thumbnail of the face
            let thumb_crop = fast_photo_ai::face::FaceModel::crop_face(&img, det, 0.4);
            let thumb_resized = thumb_crop.resize(128, 128, image::imageops::FilterType::Lanczos3);
            let mut thumb_bytes = Vec::new();
            thumb_resized
                .write_to(
                    &mut std::io::Cursor::new(&mut thumb_bytes),
                    image::ImageFormat::Jpeg,
                )
                .unwrap_or(());

            // Serialize embedding to bytes
            let embed_bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

            let _ = db::insert_face(
                &state.db,
                *photo_id,
                det.x as f64,
                det.y as f64,
                det.width as f64,
                det.height as f64,
                det.confidence as f64,
                &embed_bytes,
                &thumb_bytes,
            )
            .await;

            faces_found += 1;
        }

        processed += 1;
        if processed % 50 == 0 {
            tracing::info!(
                "Face scan progress: {} photos processed, {} faces found",
                processed,
                faces_found
            );
        }
    }

    drop(ai);

    Ok(Json(serde_json::json!({
        "processed": processed,
        "faces_found": faces_found,
    })))
}

/// Cluster faces into persons using cosine similarity
async fn cluster_faces(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let all_faces = db::get_all_face_embeddings(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if all_faces.is_empty() {
        return Ok(Json(serde_json::json!({ "persons_created": 0 })));
    }

    // Parse embeddings
    let faces_with_embed: Vec<(i64, Vec<f32>)> = all_faces
        .into_iter()
        .filter_map(|(id, bytes)| {
            if bytes.len() % 4 != 0 {
                return None;
            }
            let floats: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            Some((id, floats))
        })
        .collect();

    // Simple greedy clustering using cosine similarity
    let threshold = 0.55f32;
    let mut clusters: Vec<Vec<i64>> = Vec::new();
    let mut cluster_centroids: Vec<Vec<f32>> = Vec::new();

    for (face_id, embedding) in &faces_with_embed {
        let mut best_cluster = None;
        let mut best_sim = threshold;

        for (ci, centroid) in cluster_centroids.iter().enumerate() {
            let sim = cosine_similarity(centroid, embedding);
            if sim > best_sim {
                best_sim = sim;
                best_cluster = Some(ci);
            }
        }

        match best_cluster {
            Some(ci) => {
                clusters[ci].push(*face_id);
                // Update centroid as running average
                let n = clusters[ci].len() as f32;
                for (j, val) in cluster_centroids[ci].iter_mut().enumerate() {
                    *val = *val * ((n - 1.0) / n) + embedding[j] / n;
                }
            }
            None => {
                clusters.push(vec![*face_id]);
                cluster_centroids.push(embedding.clone());
            }
        }
    }

    // Create persons for clusters with >= 2 faces
    let mut persons_created = 0u64;
    for cluster in &clusters {
        if cluster.len() < 2 {
            continue;
        }

        let person_id = db::create_person(&state.db, auth.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        for face_id in cluster {
            let _ = db::assign_face_to_person(&state.db, *face_id, person_id).await;
        }

        let _ = db::update_person_face_count(&state.db, person_id).await;
        persons_created += 1;
    }

    tracing::info!(
        "Face clustering: {} faces → {} persons",
        faces_with_embed.len(),
        persons_created
    );

    Ok(Json(serde_json::json!({
        "persons_created": persons_created,
        "total_faces": faces_with_embed.len(),
        "total_clusters": clusters.len(),
    })))
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-12 || norm_b < 1e-12 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
