use crate::models::*;
use fast_photo_common::AppError;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

pub type DbPool = SqlitePool;

/// Initialize database pool and run migrations
pub async fn init_pool(database_url: &str) -> anyhow::Result<DbPool> {
    // Ensure the data directory exists
    if let Some(path) = database_url
        .strip_prefix("sqlite:")
        .and_then(|s| s.split('?').next())
    {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;

    run_migrations(&pool).await?;
    Ok(pool)
}

/// Run database migrations
async fn run_migrations(pool: &DbPool) -> anyhow::Result<()> {
    // ─── Migrate existing databases first ───────────────
    // Must run before CREATE INDEX which references new columns.
    // ALTER TABLE ADD COLUMN fails if column already exists → ignore errors.
    let alter_migrations = vec![
        "ALTER TABLE photos ADD COLUMN is_favorite BOOLEAN NOT NULL DEFAULT 0",
        "ALTER TABLE photos ADD COLUMN deleted_at DATETIME",
        "ALTER TABLE photos ADD COLUMN phash TEXT",
        "ALTER TABLE albums ADD COLUMN share_password TEXT",
    ];
    for sql in alter_migrations {
        let _ = sqlx::query(sql).execute(pool).await;
    }

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            username    TEXT UNIQUE NOT NULL,
            password    TEXT NOT NULL,
            role        TEXT NOT NULL DEFAULT 'user',
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS libraries (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            path        TEXT NOT NULL,
            user_id     INTEGER NOT NULL REFERENCES users(id),
            scan_status TEXT NOT NULL DEFAULT 'idle',
            photo_count INTEGER NOT NULL DEFAULT 0,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS photos (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path       TEXT UNIQUE NOT NULL,
            file_name       TEXT NOT NULL,
            file_size       INTEGER NOT NULL DEFAULT 0,
            file_hash       TEXT,
            mime_type       TEXT NOT NULL DEFAULT '',
            width           INTEGER,
            height          INTEGER,
            taken_at        DATETIME,
            latitude        REAL,
            longitude       REAL,
            camera_make     TEXT,
            camera_model    TEXT,
            library_id      INTEGER NOT NULL REFERENCES libraries(id),
            has_thumbnail   BOOLEAN NOT NULL DEFAULT 0,
            clip_embedding  BLOB,
            clip_processed  BOOLEAN NOT NULL DEFAULT 0,
            live_photo_video_path TEXT,
            duration        REAL,
            is_favorite     BOOLEAN NOT NULL DEFAULT 0,
            deleted_at      DATETIME,
            phash           TEXT,
            created_at      DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_photos_taken_at ON photos(taken_at);
        CREATE INDEX IF NOT EXISTS idx_photos_library_id ON photos(library_id);
        CREATE INDEX IF NOT EXISTS idx_photos_file_path ON photos(file_path);
        CREATE INDEX IF NOT EXISTS idx_photos_phash ON photos(phash);

        CREATE TABLE IF NOT EXISTS albums (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            name            TEXT NOT NULL,
            user_id         INTEGER NOT NULL REFERENCES users(id),
            share_token     TEXT UNIQUE,
            share_password  TEXT,
            cover_photo_id  INTEGER REFERENCES photos(id),
            photo_count     INTEGER NOT NULL DEFAULT 0,
            created_at      DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS album_photos (
            album_id    INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
            photo_id    INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
            added_at    DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (album_id, photo_id)
        );

        CREATE TABLE IF NOT EXISTS tags (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT UNIQUE NOT NULL,
            category    TEXT NOT NULL DEFAULT 'scene'
        );

        CREATE TABLE IF NOT EXISTS photo_tags (
            photo_id    INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
            tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            confidence  REAL NOT NULL DEFAULT 1.0,
            PRIMARY KEY (photo_id, tag_id)
        );

        CREATE TABLE IF NOT EXISTS persons (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT,
            user_id     INTEGER NOT NULL REFERENCES users(id),
            face_count  INTEGER NOT NULL DEFAULT 0,
            cover_face_id INTEGER,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS faces (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            photo_id    INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
            person_id   INTEGER REFERENCES persons(id) ON DELETE SET NULL,
            x           REAL NOT NULL,
            y           REAL NOT NULL,
            width       REAL NOT NULL,
            height      REAL NOT NULL,
            confidence  REAL NOT NULL DEFAULT 0.0,
            embedding   BLOB,
            thumbnail   BLOB,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
        CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);

        CREATE TABLE IF NOT EXISTS activity_logs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id     INTEGER NOT NULL REFERENCES users(id),
            action      TEXT NOT NULL,
            detail      TEXT,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_activity_user ON activity_logs(user_id);
        "#,
    )
    .execute(pool)
    .await?;

    tracing::info!("Database migrations completed successfully");
    Ok(())
}

// ─── User operations ───────────────────────────────────

pub async fn get_user_count(pool: &DbPool) -> Result<i64, AppError> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

pub async fn create_user(
    pool: &DbPool,
    user: &CreateUser,
    hashed_password: &str,
) -> Result<User, AppError> {
    let role = user.role.as_deref().unwrap_or("user");
    let result = sqlx::query_as::<_, User>(
        "INSERT INTO users (username, password, role) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(&user.username)
    .bind(hashed_password)
    .bind(role)
    .fetch_one(pool)
    .await?;
    Ok(result)
}

pub async fn get_user_by_username(pool: &DbPool, username: &str) -> Result<Option<User>, AppError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

pub async fn get_user_by_id(pool: &DbPool, id: i64) -> Result<Option<User>, AppError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

// ─── Library operations ────────────────────────────────

pub async fn create_library(
    pool: &DbPool,
    lib: &CreateLibrary,
    user_id: i64,
) -> Result<Library, AppError> {
    let result = sqlx::query_as::<_, Library>(
        "INSERT INTO libraries (name, path, user_id) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(&lib.name)
    .bind(&lib.path)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(result)
}

pub async fn get_libraries(pool: &DbPool, user_id: i64) -> Result<Vec<Library>, AppError> {
    let libs = sqlx::query_as::<_, Library>(
        "SELECT * FROM libraries WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(libs)
}

pub async fn get_library_by_id(pool: &DbPool, id: i64) -> Result<Option<Library>, AppError> {
    let lib = sqlx::query_as::<_, Library>("SELECT * FROM libraries WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(lib)
}

pub async fn delete_library(pool: &DbPool, id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM photos WHERE library_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM libraries WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_library_scan_status(
    pool: &DbPool,
    _id: i64,
    status: &str,
    count: i64,
) -> Result<(), AppError> {
    sqlx::query("UPDATE libraries SET scan_status = ?, photo_count = ? WHERE id = ?")
        .bind(status)
        .bind(count)
        .bind(_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ─── Photo operations ──────────────────────────────────

pub async fn upsert_photo(pool: &DbPool, photo: &InsertPhoto) -> Result<i64, AppError> {
    let result = sqlx::query(
        r#"INSERT INTO photos (file_path, file_name, file_size, file_hash, mime_type, width, height,
            taken_at, latitude, longitude, camera_make, camera_model, library_id) 
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(file_path) DO UPDATE SET 
            file_size = excluded.file_size,
            file_hash = excluded.file_hash,
            width = excluded.width,
            height = excluded.height,
            taken_at = excluded.taken_at,
            latitude = excluded.latitude,
            longitude = excluded.longitude,
            camera_make = excluded.camera_make,
            camera_model = excluded.camera_model
           RETURNING id"#,
    )
    .bind(&photo.file_path)
    .bind(&photo.file_name)
    .bind(photo.file_size)
    .bind(&photo.file_hash)
    .bind(&photo.mime_type)
    .bind(photo.width)
    .bind(photo.height)
    .bind(photo.taken_at)
    .bind(photo.latitude)
    .bind(photo.longitude)
    .bind(&photo.camera_make)
    .bind(&photo.camera_model)
    .bind(photo.library_id)
    .fetch_one(pool)
    .await?;

    let id: i64 = sqlx::Row::get(&result, 0);
    Ok(id)
}

pub async fn set_photo_thumbnail(pool: &DbPool, photo_id: i64) -> Result<(), AppError> {
    sqlx::query("UPDATE photos SET has_thumbnail = 1 WHERE id = ?")
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_photos_by_timeline(
    pool: &DbPool,
    library_ids: &[i64],
    offset: u32,
    limit: u32,
) -> Result<(Vec<Photo>, i64), AppError> {
    if library_ids.is_empty() {
        return Ok((vec![], 0));
    }

    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");

    let count_query = format!(
        "SELECT COUNT(*) FROM photos WHERE library_id IN ({}) AND deleted_at IS NULL",
        in_clause
    );
    let mut count_q = sqlx::query_as::<_, (i64,)>(&count_query);
    for id in library_ids {
        count_q = count_q.bind(id);
    }
    let (total,) = count_q.fetch_one(pool).await?;

    let query = format!(
        "SELECT * FROM photos WHERE library_id IN ({}) AND deleted_at IS NULL ORDER BY COALESCE(taken_at, created_at) DESC LIMIT ? OFFSET ?",
        in_clause
    );
    let mut q = sqlx::query_as::<_, Photo>(&query);
    for id in library_ids {
        q = q.bind(id);
    }
    q = q.bind(limit).bind(offset);
    let photos = q.fetch_all(pool).await?;

    Ok((photos, total))
}

pub async fn get_photos_by_folder(
    pool: &DbPool,
    folder_path: &str,
    offset: u32,
    limit: u32,
) -> Result<(Vec<Photo>, i64), AppError> {
    let pattern = format!("{}/%", folder_path);

    let (total,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM photos WHERE file_path LIKE ? AND file_path NOT LIKE ? AND deleted_at IS NULL",
    )
    .bind(&pattern)
    .bind(&format!("{}/%/%", folder_path))
    .fetch_one(pool)
    .await?;

    let photos = sqlx::query_as::<_, Photo>(
        "SELECT * FROM photos WHERE file_path LIKE ? AND file_path NOT LIKE ? AND deleted_at IS NULL ORDER BY file_name LIMIT ? OFFSET ?"
    )
        .bind(&pattern)
        .bind(&format!("{}/%/%", folder_path))
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    Ok((photos, total))
}

pub async fn get_photo_by_id(pool: &DbPool, id: i64) -> Result<Option<Photo>, AppError> {
    let photo = sqlx::query_as::<_, Photo>("SELECT * FROM photos WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(photo)
}

pub async fn search_photos(
    pool: &DbPool,
    query: &str,
    library_ids: &[i64],
    offset: u32,
    limit: u32,
) -> Result<(Vec<Photo>, i64), AppError> {
    if library_ids.is_empty() {
        return Ok((vec![], 0));
    }

    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");
    let search_pattern = format!("%{}%", query);

    let count_query = format!(
        "SELECT COUNT(*) FROM photos WHERE library_id IN ({}) AND deleted_at IS NULL AND (file_name LIKE ? OR camera_make LIKE ? OR camera_model LIKE ?)",
        in_clause
    );
    let mut count_q = sqlx::query_as::<_, (i64,)>(&count_query);
    for id in library_ids {
        count_q = count_q.bind(id);
    }
    count_q = count_q
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(&search_pattern);
    let (total,) = count_q.fetch_one(pool).await?;

    let data_query = format!(
        "SELECT * FROM photos WHERE library_id IN ({}) AND deleted_at IS NULL AND (file_name LIKE ? OR camera_make LIKE ? OR camera_model LIKE ?) ORDER BY COALESCE(taken_at, created_at) DESC LIMIT ? OFFSET ?",
        in_clause
    );
    let mut q = sqlx::query_as::<_, Photo>(&data_query);
    for id in library_ids {
        q = q.bind(id);
    }
    q = q
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(limit)
        .bind(offset);
    let photos = q.fetch_all(pool).await?;

    Ok((photos, total))
}

pub async fn get_folder_tree(
    pool: &DbPool,
    library_ids: &[i64],
) -> Result<Vec<FolderInfo>, AppError> {
    if library_ids.is_empty() {
        return Ok(vec![]);
    }

    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");

    let query = format!(
        "SELECT DISTINCT file_path FROM photos WHERE library_id IN ({})",
        in_clause
    );
    let mut q = sqlx::query_as::<_, (String,)>(&query);
    for id in library_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;

    // Build folder counts from file paths
    let mut folder_counts: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    for (path,) in &rows {
        if let Some(parent) = std::path::Path::new(path).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            *folder_counts.entry(parent_str).or_insert(0) += 1;
        }
    }

    let folders: Vec<FolderInfo> = folder_counts
        .into_iter()
        .map(|(path, count)| {
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            FolderInfo {
                name,
                path,
                photo_count: count,
            }
        })
        .collect();

    Ok(folders)
}

// ─── Geo operations ────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GeoPhoto {
    pub id: i64,
    pub file_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub taken_at: Option<NaiveDateTime>,
}

pub async fn get_geo_photos(pool: &DbPool, library_ids: &[i64]) -> Result<Vec<GeoPhoto>, AppError> {
    if library_ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");
    let query = format!(
        "SELECT id, file_name, latitude, longitude, taken_at FROM photos WHERE library_id IN ({}) AND latitude IS NOT NULL AND longitude IS NOT NULL AND deleted_at IS NULL",
        in_clause
    );
    let mut q = sqlx::query_as::<_, GeoPhoto>(&query);
    for id in library_ids {
        q = q.bind(id);
    }
    let photos = q.fetch_all(pool).await?;
    Ok(photos)
}

// ─── Album operations ──────────────────────────────────

pub async fn create_album(pool: &DbPool, name: &str, user_id: i64) -> Result<Album, AppError> {
    let share_token = uuid::Uuid::new_v4().to_string();
    let album = sqlx::query_as::<_, Album>(
        "INSERT INTO albums (name, user_id, share_token) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(name)
    .bind(user_id)
    .bind(&share_token)
    .fetch_one(pool)
    .await?;
    Ok(album)
}

pub async fn get_albums(pool: &DbPool, user_id: i64) -> Result<Vec<Album>, AppError> {
    let albums = sqlx::query_as::<_, Album>(
        "SELECT * FROM albums WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(albums)
}

pub async fn get_album_by_id(pool: &DbPool, id: i64) -> Result<Option<Album>, AppError> {
    let album = sqlx::query_as::<_, Album>("SELECT * FROM albums WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(album)
}

pub async fn get_album_by_share_token(
    pool: &DbPool,
    token: &str,
) -> Result<Option<Album>, AppError> {
    let album = sqlx::query_as::<_, Album>("SELECT * FROM albums WHERE share_token = ?")
        .bind(token)
        .fetch_optional(pool)
        .await?;
    Ok(album)
}

pub async fn delete_album(pool: &DbPool, id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM album_photos WHERE album_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM albums WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn add_photos_to_album(
    pool: &DbPool,
    album_id: i64,
    photo_ids: &[i64],
) -> Result<(), AppError> {
    for photo_id in photo_ids {
        sqlx::query("INSERT OR IGNORE INTO album_photos (album_id, photo_id) VALUES (?, ?)")
            .bind(album_id)
            .bind(photo_id)
            .execute(pool)
            .await?;
    }
    // Update count + cover
    sqlx::query(
        "UPDATE albums SET photo_count = (SELECT COUNT(*) FROM album_photos WHERE album_id = ?),
         cover_photo_id = COALESCE(cover_photo_id, (SELECT photo_id FROM album_photos WHERE album_id = ? LIMIT 1)) WHERE id = ?"
    )
        .bind(album_id).bind(album_id).bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_photo_from_album(
    pool: &DbPool,
    album_id: i64,
    photo_id: i64,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM album_photos WHERE album_id = ? AND photo_id = ?")
        .bind(album_id)
        .bind(photo_id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE albums SET photo_count = (SELECT COUNT(*) FROM album_photos WHERE album_id = ?) WHERE id = ?")
        .bind(album_id).bind(album_id).execute(pool).await?;
    Ok(())
}

pub async fn get_album_photos(
    pool: &DbPool,
    album_id: i64,
    offset: u32,
    limit: u32,
) -> Result<(Vec<Photo>, i64), AppError> {
    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM album_photos WHERE album_id = ?")
        .bind(album_id)
        .fetch_one(pool)
        .await?;

    let photos = sqlx::query_as::<_, Photo>(
        "SELECT p.* FROM photos p INNER JOIN album_photos ap ON p.id = ap.photo_id WHERE ap.album_id = ? ORDER BY ap.added_at DESC LIMIT ? OFFSET ?"
    )
        .bind(album_id).bind(limit).bind(offset)
        .fetch_all(pool).await?;

    Ok((photos, total))
}

// ─── Insert helper ─────────────────────────────────────

#[derive(Debug)]
pub struct InsertPhoto {
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
}

#[derive(Debug, Serialize)]
pub struct FolderInfo {
    pub name: String,
    pub path: String,
    pub photo_count: i64,
}

use chrono::NaiveDateTime;
use serde::Serialize;

// ─── Embedding operations ──────────────────────────────

pub async fn save_clip_embedding(
    pool: &DbPool,
    photo_id: i64,
    embedding: &[f32],
) -> Result<(), AppError> {
    // Store as raw bytes (4 bytes per f32)
    let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
    sqlx::query("UPDATE photos SET clip_embedding = ?, clip_processed = 1 WHERE id = ?")
        .bind(&bytes)
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn decode_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

// ─── Live Photos ────────────────────────────────────────

/// After scanning, pair image files with MOV files that share the same base name.
/// e.g., IMG_1234.HEIC + IMG_1234.MOV → set live_photo_video_path on the HEIC record
pub async fn pair_live_photos(pool: &DbPool, library_id: i64) -> Result<u64, AppError> {
    // Find all MOV/MP4 files in this library (potential live photo videos)
    let video_rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, file_path FROM photos WHERE library_id = ? AND (mime_type LIKE 'video/%')",
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    let mut paired = 0u64;

    for (_video_id, video_path) in &video_rows {
        let video_path_obj = std::path::Path::new(video_path);
        let video_stem = match video_path_obj.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let video_dir = video_path_obj
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Find an image file in the same directory with the same stem
        let image = sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM photos WHERE library_id = ? AND mime_type LIKE 'image/%' AND file_path LIKE ? AND file_name LIKE ? AND live_photo_video_path IS NULL"
        )
            .bind(library_id)
            .bind(format!("{}%", video_dir))
            .bind(format!("{}%", video_stem))
            .fetch_optional(pool)
            .await?;

        if let Some((image_id,)) = image {
            sqlx::query("UPDATE photos SET live_photo_video_path = ? WHERE id = ?")
                .bind(video_path)
                .bind(image_id)
                .execute(pool)
                .await?;
            paired += 1;
            tracing::debug!(
                "Paired Live Photo: image {} ← video {}",
                image_id,
                video_path
            );
        }
    }

    if paired > 0 {
        tracing::info!("Paired {} Live Photos in library {}", paired, library_id);
    }
    Ok(paired)
}

pub async fn get_photos_without_embeddings(
    pool: &DbPool,
    library_ids: &[i64],
    limit: u32,
) -> Result<Vec<Photo>, AppError> {
    if library_ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");
    let query = format!(
        "SELECT * FROM photos WHERE library_id IN ({}) AND clip_processed = 0 AND mime_type LIKE 'image/%' LIMIT ?",
        in_clause
    );
    let mut q = sqlx::query_as::<_, Photo>(&query);
    for id in library_ids {
        q = q.bind(id);
    }
    q = q.bind(limit);
    let photos = q.fetch_all(pool).await?;
    Ok(photos)
}

pub async fn get_all_embeddings(
    pool: &DbPool,
    library_ids: &[i64],
) -> Result<Vec<(i64, Vec<u8>)>, AppError> {
    if library_ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");
    let query = format!(
        "SELECT id, clip_embedding FROM photos WHERE library_id IN ({}) AND clip_processed = 1 AND clip_embedding IS NOT NULL",
        in_clause
    );
    let mut q = sqlx::query_as::<_, (i64, Vec<u8>)>(&query);
    for id in library_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

// ─── Tag operations ────────────────────────────────────

pub async fn upsert_tag(pool: &DbPool, name: &str, category: &str) -> Result<i64, AppError> {
    let result = sqlx::query(
        "INSERT INTO tags (name, category) VALUES (?, ?) ON CONFLICT(name) DO UPDATE SET category = excluded.category RETURNING id"
    )
        .bind(name)
        .bind(category)
        .fetch_one(pool)
        .await?;
    let id: i64 = sqlx::Row::get(&result, 0);
    Ok(id)
}

pub async fn add_photo_tag(
    pool: &DbPool,
    photo_id: i64,
    tag_id: i64,
    confidence: f32,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT OR REPLACE INTO photo_tags (photo_id, tag_id, confidence) VALUES (?, ?, ?)",
    )
    .bind(photo_id)
    .bind(tag_id)
    .bind(confidence)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TagWithCount {
    pub id: i64,
    pub name: String,
    pub category: String,
    pub photo_count: i64,
}

pub async fn get_tags_with_counts(pool: &DbPool) -> Result<Vec<TagWithCount>, AppError> {
    let tags = sqlx::query_as::<_, TagWithCount>(
        "SELECT t.id, t.name, t.category, COUNT(pt.photo_id) as photo_count 
         FROM tags t 
         LEFT JOIN photo_tags pt ON t.id = pt.tag_id 
         GROUP BY t.id 
         HAVING photo_count > 0 
         ORDER BY photo_count DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(tags)
}

pub async fn get_photos_by_tag(
    pool: &DbPool,
    tag_id: i64,
    offset: u32,
    limit: u32,
) -> Result<(Vec<Photo>, i64), AppError> {
    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM photo_tags WHERE tag_id = ?")
        .bind(tag_id)
        .fetch_one(pool)
        .await?;

    let photos = sqlx::query_as::<_, Photo>(
        "SELECT p.* FROM photos p 
         INNER JOIN photo_tags pt ON p.id = pt.photo_id 
         WHERE pt.tag_id = ? 
         ORDER BY pt.confidence DESC 
         LIMIT ? OFFSET ?",
    )
    .bind(tag_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((photos, total))
}

// ─── Face & Person operations ──────────────────────────

pub async fn insert_face(
    pool: &DbPool,
    photo_id: i64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    confidence: f64,
    embedding: &[u8],
    thumbnail: &[u8],
) -> Result<i64, AppError> {
    let result = sqlx::query(
        "INSERT INTO faces (photo_id, x, y, width, height, confidence, embedding, thumbnail) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(photo_id)
    .bind(x).bind(y).bind(width).bind(height)
    .bind(confidence)
    .bind(embedding)
    .bind(thumbnail)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn get_faces_for_photo(pool: &DbPool, photo_id: i64) -> Result<Vec<Face>, AppError> {
    let faces = sqlx::query_as::<_, Face>("SELECT * FROM faces WHERE photo_id = ?")
        .bind(photo_id)
        .fetch_all(pool)
        .await?;
    Ok(faces)
}

pub async fn get_all_face_embeddings(pool: &DbPool) -> Result<Vec<(i64, Vec<u8>)>, AppError> {
    let rows = sqlx::query_as::<_, (i64, Vec<u8>)>(
        "SELECT id, embedding FROM faces WHERE embedding IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn create_person(pool: &DbPool, user_id: i64) -> Result<i64, AppError> {
    let result = sqlx::query("INSERT INTO persons (user_id) VALUES (?)")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

pub async fn list_persons(pool: &DbPool, user_id: i64) -> Result<Vec<Person>, AppError> {
    let persons = sqlx::query_as::<_, Person>(
        "SELECT * FROM persons WHERE user_id = ? AND face_count > 0 ORDER BY face_count DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(persons)
}

pub async fn rename_person(pool: &DbPool, person_id: i64, name: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE persons SET name = ? WHERE id = ?")
        .bind(name)
        .bind(person_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn assign_face_to_person(
    pool: &DbPool,
    face_id: i64,
    person_id: i64,
) -> Result<(), AppError> {
    sqlx::query("UPDATE faces SET person_id = ? WHERE id = ?")
        .bind(person_id)
        .bind(face_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_person_face_count(pool: &DbPool, person_id: i64) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE persons SET face_count = (SELECT COUNT(*) FROM faces WHERE person_id = ?), 
         cover_face_id = (SELECT id FROM faces WHERE person_id = ? ORDER BY confidence DESC LIMIT 1)
         WHERE id = ?",
    )
    .bind(person_id)
    .bind(person_id)
    .bind(person_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_person_photos(
    pool: &DbPool,
    person_id: i64,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Photo>, i64), AppError> {
    let total: (i64,) =
        sqlx::query_as("SELECT COUNT(DISTINCT f.photo_id) FROM faces f WHERE f.person_id = ?")
            .bind(person_id)
            .fetch_one(pool)
            .await?;

    let photos = sqlx::query_as::<_, Photo>(
        "SELECT DISTINCT p.* FROM photos p INNER JOIN faces f ON f.photo_id = p.id WHERE f.person_id = ? ORDER BY p.taken_at DESC LIMIT ? OFFSET ?"
    )
    .bind(person_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((photos, total.0))
}

pub async fn get_face_thumbnail(pool: &DbPool, face_id: i64) -> Result<Option<Vec<u8>>, AppError> {
    let row = sqlx::query_as::<_, (Option<Vec<u8>>,)>("SELECT thumbnail FROM faces WHERE id = ?")
        .bind(face_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|r| r.0))
}

pub async fn has_faces_processed(pool: &DbPool, photo_id: i64) -> Result<bool, AppError> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM faces WHERE photo_id = ?")
        .bind(photo_id)
        .fetch_one(pool)
        .await?;
    Ok(count.0 > 0)
}

// ─── Favorites ─────────────────────────────────

pub async fn toggle_favorite(pool: &DbPool, photo_id: i64) -> Result<bool, AppError> {
    let photo = get_photo_by_id(pool, photo_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Photo not found".into()))?;
    let new_val = !photo.is_favorite;
    sqlx::query("UPDATE photos SET is_favorite = ? WHERE id = ?")
        .bind(new_val)
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(new_val)
}

pub async fn set_favorite_batch(
    pool: &DbPool,
    photo_ids: &[i64],
    favorite: bool,
) -> Result<u64, AppError> {
    let mut affected = 0u64;
    for id in photo_ids {
        sqlx::query("UPDATE photos SET is_favorite = ? WHERE id = ?")
            .bind(favorite)
            .bind(id)
            .execute(pool)
            .await?;
        affected += 1;
    }
    Ok(affected)
}

pub async fn get_favorites(
    pool: &DbPool,
    library_ids: &[i64],
    limit: i64,
    offset: i64,
) -> Result<(Vec<Photo>, i64), AppError> {
    if library_ids.is_empty() {
        return Ok((vec![], 0));
    }
    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");

    let count_query = format!(
        "SELECT COUNT(*) FROM photos WHERE library_id IN ({}) AND is_favorite = 1 AND deleted_at IS NULL",
        in_clause
    );
    let mut count_q = sqlx::query_as::<_, (i64,)>(&count_query);
    for id in library_ids {
        count_q = count_q.bind(id);
    }
    let (total,) = count_q.fetch_one(pool).await?;

    let query = format!(
        "SELECT * FROM photos WHERE library_id IN ({}) AND is_favorite = 1 AND deleted_at IS NULL ORDER BY COALESCE(taken_at, created_at) DESC LIMIT ? OFFSET ?",
        in_clause
    );
    let mut q = sqlx::query_as::<_, Photo>(&query);
    for id in library_ids {
        q = q.bind(id);
    }
    q = q.bind(limit).bind(offset);
    let photos = q.fetch_all(pool).await?;
    Ok((photos, total))
}

// ─── Trash ─────────────────────────────────────

pub async fn trash_photo(pool: &DbPool, photo_id: i64) -> Result<(), AppError> {
    sqlx::query("UPDATE photos SET deleted_at = datetime('now') WHERE id = ?")
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn trash_batch(pool: &DbPool, photo_ids: &[i64]) -> Result<u64, AppError> {
    let mut affected = 0u64;
    for id in photo_ids {
        sqlx::query("UPDATE photos SET deleted_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        affected += 1;
    }
    Ok(affected)
}

pub async fn restore_photo(pool: &DbPool, photo_id: i64) -> Result<(), AppError> {
    sqlx::query("UPDATE photos SET deleted_at = NULL WHERE id = ?")
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn restore_batch(pool: &DbPool, photo_ids: &[i64]) -> Result<u64, AppError> {
    let mut affected = 0u64;
    for id in photo_ids {
        sqlx::query("UPDATE photos SET deleted_at = NULL WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        affected += 1;
    }
    Ok(affected)
}

pub async fn permanent_delete(pool: &DbPool, photo_id: i64) -> Result<String, AppError> {
    let photo = get_photo_by_id(pool, photo_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Photo not found".into()))?;
    sqlx::query("DELETE FROM photos WHERE id = ?")
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(photo.file_path)
}

pub async fn get_trash(
    pool: &DbPool,
    library_ids: &[i64],
    limit: i64,
    offset: i64,
) -> Result<(Vec<Photo>, i64), AppError> {
    if library_ids.is_empty() {
        return Ok((vec![], 0));
    }
    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");

    let count_query = format!(
        "SELECT COUNT(*) FROM photos WHERE library_id IN ({}) AND deleted_at IS NOT NULL",
        in_clause
    );
    let mut count_q = sqlx::query_as::<_, (i64,)>(&count_query);
    for id in library_ids {
        count_q = count_q.bind(id);
    }
    let (total,) = count_q.fetch_one(pool).await?;

    let query = format!(
        "SELECT * FROM photos WHERE library_id IN ({}) AND deleted_at IS NOT NULL ORDER BY deleted_at DESC LIMIT ? OFFSET ?",
        in_clause
    );
    let mut q = sqlx::query_as::<_, Photo>(&query);
    for id in library_ids {
        q = q.bind(id);
    }
    q = q.bind(limit).bind(offset);
    let photos = q.fetch_all(pool).await?;
    Ok((photos, total))
}

pub async fn empty_trash(pool: &DbPool, library_ids: &[i64]) -> Result<Vec<String>, AppError> {
    if library_ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");

    let query = format!(
        "SELECT file_path FROM photos WHERE library_id IN ({}) AND deleted_at IS NOT NULL",
        in_clause
    );
    let mut q = sqlx::query_as::<_, (String,)>(&query);
    for id in library_ids {
        q = q.bind(id);
    }
    let paths: Vec<String> = q.fetch_all(pool).await?.into_iter().map(|r| r.0).collect();

    let del_query = format!(
        "DELETE FROM photos WHERE library_id IN ({}) AND deleted_at IS NOT NULL",
        in_clause
    );
    let mut dq = sqlx::query(&del_query);
    for id in library_ids {
        dq = dq.bind(id);
    }
    dq.execute(pool).await?;

    Ok(paths)
}

// ─── Duplicates ────────────────────────────────

pub async fn update_phash(pool: &DbPool, photo_id: i64, phash: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE photos SET phash = ? WHERE id = ?")
        .bind(phash)
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_duplicates(
    pool: &DbPool,
    library_ids: &[i64],
) -> Result<Vec<(String, Vec<Photo>)>, AppError> {
    if library_ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");

    // Find phash values with more than 1 photo
    let dup_query = format!(
        "SELECT phash, COUNT(*) as cnt FROM photos WHERE library_id IN ({}) AND phash IS NOT NULL AND deleted_at IS NULL GROUP BY phash HAVING cnt > 1 ORDER BY cnt DESC LIMIT 100",
        in_clause
    );
    let mut dq = sqlx::query_as::<_, (String, i64)>(&dup_query);
    for id in library_ids {
        dq = dq.bind(id);
    }
    let dup_hashes = dq.fetch_all(pool).await?;

    let mut result = Vec::new();
    for (phash, _count) in dup_hashes {
        let photos_query = format!(
            "SELECT * FROM photos WHERE phash = ? AND library_id IN ({}) AND deleted_at IS NULL ORDER BY created_at",
            in_clause
        );
        let mut pq = sqlx::query_as::<_, Photo>(&photos_query);
        pq = pq.bind(&phash);
        for id in library_ids {
            pq = pq.bind(id);
        }
        let photos = pq.fetch_all(pool).await?;
        result.push((phash, photos));
    }

    Ok(result)
}

// ─── Upload ────────────────────────────────────

pub async fn insert_uploaded_photo(
    pool: &DbPool,
    file_path: &str,
    file_name: &str,
    file_size: i64,
    mime_type: &str,
    library_id: i64,
    file_hash: Option<&str>,
) -> Result<i64, AppError> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO photos (file_path, file_name, file_size, mime_type, library_id, file_hash) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(file_path)
    .bind(file_name)
    .bind(file_size)
    .bind(mime_type)
    .bind(library_id)
    .bind(file_hash)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

// ─── Tags ──────────────────────────────────────

pub async fn get_or_create_tag(pool: &DbPool, name: &str, category: &str) -> Result<i64, AppError> {
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    if let Some((id,)) = existing {
        return Ok(id);
    }
    let result = sqlx::query("INSERT INTO tags (name, category) VALUES (?, ?)")
        .bind(name)
        .bind(category)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

pub async fn add_tag_to_photo(
    pool: &DbPool,
    photo_id: i64,
    tag_id: i64,
    confidence: f64,
) -> Result<(), AppError> {
    sqlx::query("INSERT OR IGNORE INTO photo_tags (photo_id, tag_id, confidence) VALUES (?, ?, ?)")
        .bind(photo_id)
        .bind(tag_id)
        .bind(confidence)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_tag_from_photo(
    pool: &DbPool,
    photo_id: i64,
    tag_id: i64,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM photo_tags WHERE photo_id = ? AND tag_id = ?")
        .bind(photo_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_photo_tags(pool: &DbPool, photo_id: i64) -> Result<Vec<Tag>, AppError> {
    let tags = sqlx::query_as::<_, Tag>(
        "SELECT t.* FROM tags t INNER JOIN photo_tags pt ON t.id = pt.tag_id WHERE pt.photo_id = ? ORDER BY t.name"
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await?;
    Ok(tags)
}

pub async fn list_all_tags(pool: &DbPool) -> Result<Vec<Tag>, AppError> {
    let tags = sqlx::query_as::<_, Tag>("SELECT * FROM tags ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(tags)
}

pub async fn search_photos_by_tag(
    pool: &DbPool,
    tag_name: &str,
    library_ids: &[i64],
    limit: i64,
    offset: i64,
) -> Result<(Vec<Photo>, i64), AppError> {
    if library_ids.is_empty() {
        return Ok((vec![], 0));
    }
    let placeholders: Vec<String> = library_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");

    let count_query = format!(
        "SELECT COUNT(*) FROM photos p INNER JOIN photo_tags pt ON p.id = pt.photo_id INNER JOIN tags t ON t.id = pt.tag_id WHERE t.name = ? AND p.library_id IN ({}) AND p.deleted_at IS NULL",
        in_clause
    );
    let mut cq = sqlx::query_as::<_, (i64,)>(&count_query);
    cq = cq.bind(tag_name);
    for id in library_ids {
        cq = cq.bind(id);
    }
    let (total,) = cq.fetch_one(pool).await?;

    let data_query = format!(
        "SELECT p.* FROM photos p INNER JOIN photo_tags pt ON p.id = pt.photo_id INNER JOIN tags t ON t.id = pt.tag_id WHERE t.name = ? AND p.library_id IN ({}) AND p.deleted_at IS NULL ORDER BY p.taken_at DESC LIMIT ? OFFSET ?",
        in_clause
    );
    let mut dq = sqlx::query_as::<_, Photo>(&data_query);
    dq = dq.bind(tag_name);
    for id in library_ids {
        dq = dq.bind(id);
    }
    dq = dq.bind(limit).bind(offset);
    let photos = dq.fetch_all(pool).await?;
    Ok((photos, total))
}

// ─── Activity Log ──────────────────────────────

pub async fn log_activity(
    pool: &DbPool,
    user_id: i64,
    action: &str,
    detail: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO activity_logs (user_id, action, detail) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(action)
        .bind(detail)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_activity_logs(
    pool: &DbPool,
    user_id: i64,
    limit: i64,
) -> Result<Vec<ActivityLog>, AppError> {
    let logs = sqlx::query_as::<_, ActivityLog>(
        "SELECT * FROM activity_logs WHERE user_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(logs)
}

// ─── Share Password ────────────────────────────

pub async fn set_share_password(
    pool: &DbPool,
    album_id: i64,
    password: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query("UPDATE albums SET share_password = ? WHERE id = ?")
        .bind(password)
        .bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_album_share_password(
    pool: &DbPool,
    album_id: i64,
) -> Result<Option<String>, AppError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT share_password FROM albums WHERE id = ?")
            .bind(album_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0))
}
