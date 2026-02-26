use crate::db::{self, DbPool, InsertPhoto};
use crate::exif;
use crate::models::ScanProgress;
use sqlx;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::watch;
use tracing;
use walkdir::WalkDir;

const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff", "tif", "heic", "heif", "avif", "svg",
];

const SUPPORTED_VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm", "m4v", "3gp"];

/// Check if a file extension is a supported media type
pub fn is_supported_media(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_lowercase();
            SUPPORTED_IMAGE_EXTENSIONS.contains(&lower.as_str())
                || SUPPORTED_VIDEO_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

pub fn mime_type_from_extension(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        "heic" | "heif" => "image/heic",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "m4v" => "video/x-m4v",
        "3gp" => "video/3gpp",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Scan a directory for media files and index them into the database
pub async fn scan_library(
    pool: Arc<DbPool>,
    library_id: i64,
    library_path: &str,
    progress_tx: watch::Sender<ScanProgress>,
) -> anyhow::Result<u64> {
    let path = Path::new(library_path);
    if !path.exists() {
        anyhow::bail!("Library path does not exist: {}", library_path);
    }

    tracing::info!("Scanning library {} at {}", library_id, library_path);

    // Update status to scanning
    db::update_library_scan_status(&pool, library_id, "scanning", 0).await?;

    // Collect all media files first
    let mut media_files: Vec<std::path::PathBuf> = Vec::new();
    for entry in WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() && is_supported_media(entry.path()) {
            media_files.push(entry.path().to_path_buf());
        }
    }

    let total = media_files.len() as u64;
    tracing::info!("Found {} media files in {}", total, library_path);

    let _ = progress_tx.send(ScanProgress {
        library_id,
        total_files: total,
        processed_files: 0,
        status: "scanning".to_string(),
    });

    let mut processed: u64 = 0;

    for file_path in &media_files {
        match process_file(&pool, file_path, library_id).await {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("Failed to process {:?}: {}", file_path, e);
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
            tracing::info!("Progress: {}/{}", processed, total);
        }
    }

    // Pair Live Photos (HEIC/JPG + MOV with same basename)
    match db::pair_live_photos(&pool, library_id).await {
        Ok(paired) => {
            if paired > 0 {
                tracing::info!("Paired {} Live Photos", paired);
            }
        }
        Err(e) => tracing::warn!("Live Photo pairing failed: {}", e),
    }

    // Update status to completed
    db::update_library_scan_status(&pool, library_id, "completed", processed as i64).await?;

    let _ = progress_tx.send(ScanProgress {
        library_id,
        total_files: total,
        processed_files: processed,
        status: "completed".to_string(),
    });

    tracing::info!("Scan completed: {} files processed", processed);
    Ok(processed)
}

async fn process_file(pool: &DbPool, file_path: &Path, library_id: i64) -> anyhow::Result<()> {
    process_file_with_stored_path(pool, file_path, &file_path.to_string_lossy(), library_id).await
}

/// Index a media file from `source_path` into DB while storing `stored_path` as canonical file path.
/// For local scanning, `source_path` and `stored_path` are usually the same.
/// For remote scanning, `source_path` points to a local cache file while `stored_path` is remote key.
pub async fn process_file_with_stored_path(
    pool: &DbPool,
    source_path: &Path,
    stored_path: &str,
    library_id: i64,
) -> anyhow::Result<()> {
    use crate::thumbnailer;

    let metadata = std::fs::metadata(source_path)?;
    let file_name = Path::new(stored_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mime = mime_type_from_extension(Path::new(stored_path));
    let file_size = metadata.len() as i64;
    let is_video = mime.starts_with("video/");

    // Calculate file hash
    let file_hash = compute_file_hash(source_path)?;

    // Extract EXIF data (images only)
    let exif_data = if !is_video {
        exif::extract_exif(source_path)
    } else {
        None
    };

    // Get dimensions
    let (width, height) = if is_video {
        thumbnailer::get_video_dimensions(source_path)
            .map(|(w, h)| (Some(w), Some(h)))
            .unwrap_or((None, None))
    } else if mime.starts_with("image/") {
        exif::get_image_dimensions(source_path)
            .map(|(w, h)| (Some(w as i32), Some(h as i32)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    let insert = InsertPhoto {
        file_path: stored_path.to_string(),
        file_name,
        file_size,
        file_hash: Some(file_hash.clone()),
        mime_type: mime.clone(),
        width,
        height,
        taken_at: exif_data.as_ref().and_then(|e| e.taken_at),
        latitude: exif_data.as_ref().and_then(|e| e.latitude),
        longitude: exif_data.as_ref().and_then(|e| e.longitude),
        camera_make: exif_data.as_ref().and_then(|e| e.camera_make.clone()),
        camera_model: exif_data.as_ref().and_then(|e| e.camera_model.clone()),
        library_id,
    };

    let photo_id = db::upsert_photo(pool, &insert).await?;

    if mime.starts_with("image/") {
        let phash = match crate::dedup::compute_dhash(source_path) {
            Ok(phash) => Some(phash),
            Err(e) => {
                // Fallback to exact hash so duplicate detection still works for unsupported images.
                tracing::debug!("Failed to compute dHash for {:?}: {}", source_path, e);
                Some(file_hash.clone())
            }
        };
        if let Some(phash) = phash {
            let _ = db::update_phash(pool, photo_id, &phash).await;
        }
    }

    // For videos, extract duration and update
    if is_video {
        let duration = thumbnailer::get_video_duration(source_path);
        if duration.is_some() {
            let _ = sqlx::query("UPDATE photos SET duration = ? WHERE id = ?")
                .bind(duration)
                .bind(photo_id)
                .execute(pool)
                .await;
        }
    }

    Ok(())
}

fn compute_file_hash(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    // Only hash first 64KB for speed on large files
    let mut total_read = 0usize;
    const MAX_HASH_BYTES: usize = 65536;

    loop {
        let bytes_to_read = std::cmp::min(buffer.len(), MAX_HASH_BYTES - total_read);
        if bytes_to_read == 0 {
            break;
        }
        let n = file.read(&mut buffer[..bytes_to_read])?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total_read += n;
    }

    Ok(hex::encode(hasher.finalize()))
}
