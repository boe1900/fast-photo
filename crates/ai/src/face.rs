use anyhow::Result;
use image::DynamicImage;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;

use crate::processor;

/// Bounding box for a detected face
#[derive(Debug, Clone)]
pub struct FaceDetection {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub confidence: f32,
}

/// Face recognition model: detection + embedding
pub struct FaceModel {
    detect_session: Mutex<Session>,
    embed_session: Mutex<Session>,
}

// Safety: Sessions are protected by Mutex
unsafe impl Send for FaceModel {}
unsafe impl Sync for FaceModel {}

impl FaceModel {
    /// Load face detection (UltraFace) and embedding (MobileFaceNet) models
    pub fn load(detect_path: &Path, embed_path: &Path) -> Result<Self> {
        tracing::info!("Loading face detection model from {:?}", detect_path);
        let detect_session = Session::builder()?
            .with_intra_threads(4)?
            .commit_from_file(detect_path)?;

        tracing::info!("Loading face embedding model from {:?}", embed_path);
        let embed_session = Session::builder()?
            .with_intra_threads(4)?
            .commit_from_file(embed_path)?;

        Ok(Self {
            detect_session: Mutex::new(detect_session),
            embed_session: Mutex::new(embed_session),
        })
    }

    /// Detect faces in an image
    /// Returns list of face bounding boxes (normalized 0-1 coordinates)
    pub fn detect_faces(&self, img: &DynamicImage) -> Result<Vec<FaceDetection>> {
        let (orig_w, orig_h) = (img.width(), img.height());
        let input_w: u32 = 320;
        let input_h: u32 = 240;

        let resized = img.resize_exact(input_w, input_h, image::imageops::FilterType::Lanczos3);
        let rgb = resized.to_rgb8();

        // Preprocess: [1, 3, 240, 320], normalize to [-1, 1]
        let mut data = vec![0.0f32; (3 * input_h * input_w) as usize];
        for y in 0..input_h {
            for x in 0..input_w {
                let pixel = rgb.get_pixel(x, y);
                for c in 0..3usize {
                    let val = pixel[c] as f32 / 127.5 - 1.0; // normalize to [-1, 1]
                    let idx = c * (input_h * input_w) as usize
                        + y as usize * input_w as usize
                        + x as usize;
                    data[idx] = val;
                }
            }
        }

        let input =
            Tensor::<f32>::from_array(([1usize, 3, input_h as usize, input_w as usize], data))?;

        let mut session = self
            .detect_session
            .lock()
            .map_err(|e| anyhow::anyhow!("Session lock error: {}", e))?;
        let outputs = session.run(ort::inputs![input])?;

        // UltraFace outputs: [confidences, boxes]
        // confidences: [1, N, 2] (background, face)
        // boxes: [1, N, 4] (x1, y1, x2, y2 normalized)
        let confidences = &outputs[0];
        let boxes = &outputs[1];

        let (conf_shape, conf_data) = confidences.try_extract_tensor::<f32>()?;
        let (_, box_data) = boxes.try_extract_tensor::<f32>()?;

        let num_faces = conf_shape[1] as usize;
        let mut detections = Vec::new();
        let min_confidence = 0.7f32;

        for i in 0..num_faces {
            let face_conf = conf_data[i * 2 + 1]; // face confidence
            if face_conf < min_confidence {
                continue;
            }

            let x1 = box_data[i * 4].clamp(0.0, 1.0);
            let y1 = box_data[i * 4 + 1].clamp(0.0, 1.0);
            let x2 = box_data[i * 4 + 2].clamp(0.0, 1.0);
            let y2 = box_data[i * 4 + 3].clamp(0.0, 1.0);

            let w = x2 - x1;
            let h = y2 - y1;

            // Filter too small faces (less than 2% of image)
            if w < 0.02 || h < 0.02 {
                continue;
            }

            detections.push(FaceDetection {
                x: x1,
                y: y1,
                width: w,
                height: h,
                confidence: face_conf,
            });
        }

        // Non-maximum suppression
        detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        let detections = nms(detections, 0.3);

        let _ = (orig_w, orig_h); // used for reference only
        Ok(detections)
    }

    /// Extract face embedding from a cropped face image
    /// Input should be a tight crop of a face, will be resized to 112x112
    pub fn extract_embedding(&self, face_img: &DynamicImage) -> Result<Vec<f32>> {
        let input_size = 112;
        let resized = face_img.resize_exact(
            input_size,
            input_size,
            image::imageops::FilterType::Lanczos3,
        );
        let rgb = resized.to_rgb8();

        // Preprocess: [1, 3, 112, 112], normalize
        let mut data = vec![0.0f32; (3 * input_size * input_size) as usize];
        for y in 0..input_size {
            for x in 0..input_size {
                let pixel = rgb.get_pixel(x, y);
                for c in 0..3usize {
                    let val = (pixel[c] as f32 - 127.5) / 127.5;
                    let idx = c * (input_size * input_size) as usize
                        + y as usize * input_size as usize
                        + x as usize;
                    data[idx] = val;
                }
            }
        }

        let input = Tensor::<f32>::from_array((
            [1usize, 3, input_size as usize, input_size as usize],
            data,
        ))?;

        let mut session = self
            .embed_session
            .lock()
            .map_err(|e| anyhow::anyhow!("Session lock error: {}", e))?;
        let outputs = session.run(ort::inputs![input])?;

        let output = &outputs[0];
        let (_shape, data) = output.try_extract_tensor::<f32>()?;
        let embedding = data.to_vec();

        Ok(processor::l2_normalize(&embedding))
    }

    /// Crop a face from an image using normalized coordinates
    pub fn crop_face(img: &DynamicImage, det: &FaceDetection, padding: f32) -> DynamicImage {
        let (iw, ih) = (img.width() as f32, img.height() as f32);

        // Add padding around face
        let pad_w = det.width * padding;
        let pad_h = det.height * padding;

        let x1 = ((det.x - pad_w) * iw).max(0.0) as u32;
        let y1 = ((det.y - pad_h) * ih).max(0.0) as u32;
        let x2 = ((det.x + det.width + pad_w) * iw).min(iw) as u32;
        let y2 = ((det.y + det.height + pad_h) * ih).min(ih) as u32;

        let w = (x2 - x1).max(1);
        let h = (y2 - y1).max(1);

        img.crop_imm(x1, y1, w, h)
    }
}

/// Non-maximum suppression
fn nms(detections: Vec<FaceDetection>, iou_threshold: f32) -> Vec<FaceDetection> {
    let mut result = Vec::new();

    for det in &detections {
        let mut suppress = false;
        for kept in &result {
            if iou(det, kept) > iou_threshold {
                suppress = true;
                break;
            }
        }
        if !suppress {
            result.push(det.clone());
        }
    }

    result
}

/// Intersection over Union
fn iou(a: &FaceDetection, b: &FaceDetection) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);

    if x2 <= x1 || y2 <= y1 {
        return 0.0;
    }

    let intersection = (x2 - x1) * (y2 - y1);
    let area_a = a.width * a.height;
    let area_b = b.width * b.height;
    let union = area_a + area_b - intersection;

    if union < 1e-6 {
        return 0.0;
    }

    intersection / union
}
