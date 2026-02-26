use image::imageops::FilterType;
use image::DynamicImage;
use std::path::Path;

/// Compute an 8x8 dHash and return as fixed 16-char lowercase hex string.
pub fn compute_dhash(path: &Path) -> anyhow::Result<String> {
    let img = image::open(path)?;
    Ok(compute_dhash_from_image(&img))
}

/// Compute perceptual hash from an in-memory image.
pub fn compute_dhash_from_image(img: &DynamicImage) -> String {
    let small = img
        .grayscale()
        .resize_exact(9, 8, FilterType::Triangle)
        .to_luma8();

    let mut hash: u64 = 0;
    for y in 0..8 {
        for x in 0..8 {
            hash <<= 1;
            if small.get_pixel(x, y)[0] > small.get_pixel(x + 1, y)[0] {
                hash |= 1;
            }
        }
    }
    format!("{:016x}", hash)
}
