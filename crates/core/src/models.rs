use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

// ─── User ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub role: String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub password: String,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub role: String,
}

// ─── Library ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub user_id: i64,
    pub scan_status: String,
    pub photo_count: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateLibrary {
    pub name: String,
    pub path: String,
}

// ─── Photo ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Photo {
    pub id: i64,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub file_hash: Option<String>,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub taken_at: Option<NaiveDateTime>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub library_id: i64,
    pub has_thumbnail: bool,
    pub clip_processed: bool,
    pub live_photo_video_path: Option<String>,
    pub duration: Option<f64>,
    pub is_favorite: bool,
    pub deleted_at: Option<NaiveDateTime>,
    pub phash: Option<String>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct PhotoDetail {
    pub photo: Photo,
    pub exif_data: Option<ExifData>,
    pub thumbnail_url: String,
    pub full_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExifData {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub focal_length: Option<String>,
    pub aperture: Option<String>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
    pub taken_at: Option<NaiveDateTime>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub orientation: Option<u16>,
}

// ─── Album ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Album {
    pub id: i64,
    pub name: String,
    pub user_id: i64,
    pub share_token: Option<String>,
    pub share_password: Option<String>,
    pub cover_photo_id: Option<i64>,
    pub photo_count: i64,
    pub created_at: NaiveDateTime,
}

// ─── Activity Log ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ActivityLog {
    pub id: i64,
    pub user_id: i64,
    pub action: String,
    pub detail: Option<String>,
    pub created_at: NaiveDateTime,
}

// ─── Tag ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub category: String,
}

// ─── Person (face cluster) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Person {
    pub id: i64,
    pub name: Option<String>,
    pub user_id: i64,
    pub face_count: i64,
    pub cover_face_id: Option<i64>,
    pub created_at: NaiveDateTime,
}

// ─── Face detection ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Face {
    pub id: i64,
    pub photo_id: i64,
    pub person_id: Option<i64>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub confidence: f64,
    #[serde(skip)]
    pub embedding: Option<Vec<u8>>,
    #[serde(skip)]
    pub thumbnail: Option<Vec<u8>>,
    pub created_at: NaiveDateTime,
}

// ─── Pagination ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

impl PaginationParams {
    pub fn offset(&self) -> u32 {
        let page = self.page.unwrap_or(1).max(1);
        let per_page = self.per_page();
        (page - 1) * per_page
    }

    pub fn per_page(&self) -> u32 {
        self.per_page.unwrap_or(50).min(200)
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

// ─── Timeline group ────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TimelineGroup {
    pub date: String,
    pub photos: Vec<Photo>,
}

// ─── Folder tree ───────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct FolderNode {
    pub name: String,
    pub path: String,
    pub photo_count: i64,
    pub children: Vec<FolderNode>,
}

// ─── Scan status ───────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct ScanProgress {
    pub library_id: i64,
    pub total_files: u64,
    pub processed_files: u64,
    pub status: String,
}
