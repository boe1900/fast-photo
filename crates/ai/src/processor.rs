use image::DynamicImage;

/// CLIP image preprocessing: resize to 224x224, normalize with CLIP stats
/// Returns flattened NCHW data as Vec<f32> [1 * 3 * 224 * 224]
pub fn preprocess_image_flat(img: &DynamicImage) -> Vec<f32> {
    // Resize to 224x224 (CLIP input size)
    let resized = img.resize_exact(224, 224, image::imageops::FilterType::Lanczos3);
    let rgb = resized.to_rgb8();

    // CLIP normalization: mean=[0.48145466, 0.4578275, 0.40821073], std=[0.26862954, 0.26130258, 0.27577711]
    let mean = [0.48145466_f32, 0.4578275, 0.40821073];
    let std_dev = [0.26862954_f32, 0.26130258, 0.27577711];

    // Create flat NCHW data [1, 3, 224, 224] = 150528 floats
    let mut data = vec![0.0f32; 1 * 3 * 224 * 224];

    for y in 0..224 {
        for x in 0..224 {
            let pixel = rgb.get_pixel(x as u32, y as u32);
            for c in 0..3 {
                let val = pixel[c] as f32 / 255.0;
                let idx = c * 224 * 224 + y * 224 + x; // NCHW layout (N=0)
                data[idx] = (val - mean[c]) / std_dev[c];
            }
        }
    }

    data
}

/// Normalize a vector to unit length (L2 normalization)
pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1e-12 {
        return v.to_vec();
    }
    v.iter().map(|x| x / norm).collect()
}

/// Cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-12 || norm_b < 1e-12 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
