use image::imageops::FilterType;
use image::DynamicImage;
use std::path::{Path, PathBuf};
use tracing;

/// Thumbnail sizes
#[derive(Debug, Clone, Copy)]
pub enum ThumbnailSize {
    Small,  // 256px - for grids
    Medium, // 720px - for previews
    Large,  // 1920px - for full view
}

impl ThumbnailSize {
    pub fn max_dimension(&self) -> u32 {
        match self {
            ThumbnailSize::Small => 256,
            ThumbnailSize::Medium => 720,
            ThumbnailSize::Large => 1920,
        }
    }

    pub fn dir_name(&self) -> &str {
        match self {
            ThumbnailSize::Small => "small",
            ThumbnailSize::Medium => "medium",
            ThumbnailSize::Large => "large",
        }
    }
}

/// Get the thumbnail path for a photo
pub fn thumbnail_path(thumb_dir: &Path, photo_id: i64, size: ThumbnailSize) -> PathBuf {
    // Shard by id to avoid too many files in one directory
    let shard = photo_id % 1000;
    thumb_dir
        .join(size.dir_name())
        .join(format!("{:03}", shard))
        .join(format!("{}.webp", photo_id))
}

/// Generate thumbnails for a photo in all sizes
pub fn generate_thumbnails(
    source_path: &Path,
    thumb_dir: &Path,
    photo_id: i64,
) -> anyhow::Result<()> {
    let img = image::open(source_path)?;

    for size in &[
        ThumbnailSize::Small,
        ThumbnailSize::Medium,
        ThumbnailSize::Large,
    ] {
        let out_path = thumbnail_path(thumb_dir, photo_id, *size);

        // Skip if already exists
        if out_path.exists() {
            continue;
        }

        // Create parent directories
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let thumb = resize_image(&img, size.max_dimension());
        thumb.save_with_format(&out_path, image::ImageFormat::WebP)?;

        tracing::debug!("Generated {:?} thumbnail for photo {}", size, photo_id);
    }

    Ok(())
}

/// Generate a single thumbnail size
pub fn generate_thumbnail(
    source_path: &Path,
    thumb_dir: &Path,
    photo_id: i64,
    size: ThumbnailSize,
) -> anyhow::Result<PathBuf> {
    let out_path = thumbnail_path(thumb_dir, photo_id, size);

    if out_path.exists() {
        return Ok(out_path);
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let img = image::open(source_path)?;
    let thumb = resize_image(&img, size.max_dimension());
    thumb.save_with_format(&out_path, image::ImageFormat::WebP)?;

    Ok(out_path)
}

fn resize_image(img: &DynamicImage, max_dim: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim {
        return img.clone();
    }
    img.resize(max_dim, max_dim, FilterType::Lanczos3)
}

/// Generate thumbnail for a video file using ffmpeg
pub fn generate_video_thumbnail(
    source_path: &Path,
    thumb_dir: &Path,
    photo_id: i64,
) -> anyhow::Result<()> {
    // Extract frame at 1 second mark
    let temp_frame = thumb_dir.join(format!("_tmp_frame_{}.jpg", photo_id));

    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            &source_path.to_string_lossy(),
            "-ss",
            "1",
            "-vframes",
            "1",
            "-q:v",
            "2",
            &temp_frame.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() && temp_frame.exists() => {
            // Now generate thumbnails from the extracted frame
            let result = generate_thumbnails(&temp_frame, thumb_dir, photo_id);
            let _ = std::fs::remove_file(&temp_frame);
            result
        }
        _ => {
            let _ = std::fs::remove_file(&temp_frame);
            anyhow::bail!("ffmpeg failed to extract video frame")
        }
    }
}

/// Extract video duration using ffprobe
pub fn get_video_duration(source_path: &Path) -> Option<f64> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
            &source_path.to_string_lossy(),
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        text.trim().parse::<f64>().ok()
    } else {
        None
    }
}

/// Get video dimensions using ffprobe
pub fn get_video_dimensions(source_path: &Path) -> Option<(i32, i32)> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
            &source_path.to_string_lossy(),
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text.lines().next()?.trim();
        let parts: Vec<&str> = line.split('x').collect();
        if parts.len() == 2 {
            let w = parts[0].parse().ok()?;
            let h = parts[1].parse().ok()?;
            return Some((w, h));
        }
    }
    None
}
