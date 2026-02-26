use anyhow::Result;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;
use tokenizers::Tokenizer;

use crate::processor;

/// CLIP ViT-B-32 model for image-text embeddings
pub struct ClipModel {
    vision_session: Mutex<Session>,
    text_session: Mutex<Session>,
    tokenizer: Tokenizer,
}

// Safety: Sessions are protected by Mutex
unsafe impl Send for ClipModel {}
unsafe impl Sync for ClipModel {}

impl ClipModel {
    /// Load CLIP ONNX models and tokenizer
    pub fn load(vision_path: &Path, text_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        tracing::info!("Loading CLIP vision model from {:?}", vision_path);
        let vision_session = Session::builder()?
            .with_intra_threads(4)?
            .commit_from_file(vision_path)?;

        tracing::info!("Loading CLIP text model from {:?}", text_path);
        let text_session = Session::builder()?
            .with_intra_threads(4)?
            .commit_from_file(text_path)?;

        tracing::info!("Loading tokenizer from {:?}", tokenizer_path);
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Tokenizer load error: {}", e))?;

        Ok(Self {
            vision_session: Mutex::new(vision_session),
            text_session: Mutex::new(text_session),
            tokenizer,
        })
    }

    /// Encode an image to a 512-dim embedding
    pub fn encode_image(&self, img: &image::DynamicImage) -> Result<Vec<f32>> {
        let tensor_data = processor::preprocess_image_flat(img);

        // Use (shape, vec) tuple format
        let input = Tensor::<f32>::from_array(([1usize, 3, 224, 224], tensor_data))?;

        let mut session = self
            .vision_session
            .lock()
            .map_err(|e| anyhow::anyhow!("Session lock error: {}", e))?;
        let outputs = session.run(ort::inputs![input])?;

        let output = &outputs[0];
        let (_shape, data) = output.try_extract_tensor::<f32>()?;
        let embedding = data.to_vec();

        Ok(processor::l2_normalize(&embedding))
    }

    /// Encode a text query to a 512-dim embedding
    pub fn encode_text(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("Tokenize error: {}", e))?;

        let ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();

        // Pad/truncate to 77 tokens (CLIP max length)
        let max_len = 77;
        let mut input_ids = vec![0i64; max_len];
        let mut mask = vec![0i64; max_len];

        for i in 0..ids.len().min(max_len) {
            input_ids[i] = ids[i] as i64;
            mask[i] = attention_mask[i] as i64;
        }

        let input_ids_tensor = Tensor::<i64>::from_array(([1usize, max_len], input_ids))?;
        let mask_tensor = Tensor::<i64>::from_array(([1usize, max_len], mask))?;

        let mut session = self
            .text_session
            .lock()
            .map_err(|e| anyhow::anyhow!("Session lock error: {}", e))?;
        let outputs = session.run(ort::inputs![input_ids_tensor, mask_tensor])?;

        let output = &outputs[0];
        let (_shape, data) = output.try_extract_tensor::<f32>()?;
        let embedding = data.to_vec();

        Ok(processor::l2_normalize(&embedding))
    }

    /// Compute similarity between an image and a text query
    pub fn similarity(&self, image_embedding: &[f32], text: &str) -> Result<f32> {
        let text_embedding = self.encode_text(text)?;
        Ok(processor::cosine_similarity(
            image_embedding,
            &text_embedding,
        ))
    }

    /// Classify an image using CLIP zero-shot classification
    /// Returns sorted (label, score) pairs
    pub fn classify(&self, image_embedding: &[f32], labels: &[&str]) -> Result<Vec<(String, f32)>> {
        let mut results: Vec<(String, f32)> = Vec::new();

        for label in labels {
            let prompt = format!("a photo of {}", label);
            let text_embedding = self.encode_text(&prompt)?;
            let score = processor::cosine_similarity(image_embedding, &text_embedding);
            results.push((label.to_string(), score));
        }

        // Sort by score descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(results)
    }
}

/// Predefined scene labels for zero-shot classification
pub const SCENE_LABELS: &[&str] = &[
    "landscape",    // 风景
    "portrait",     // 人像
    "food",         // 美食
    "animal",       // 动物
    "architecture", // 建筑
    "beach",        // 海滩
    "mountain",     // 山
    "sunset",       // 日落
    "night",        // 夜景
    "flower",       // 花
    "city",         // 城市
    "forest",       // 森林
    "snow",         // 雪景
    "water",        // 水景
    "car",          // 汽车
    "text",         // 文字/截图
    "indoor",       // 室内
    "group photo",  // 合照
    "selfie",       // 自拍
    "screenshot",   // 截图
    "document",     // 文档
    "artwork",      // 艺术品
    "sport",        // 运动
    "concert",      // 演唱会
    "celebration",  // 庆典
];

/// Scene label to Chinese display name
pub fn scene_label_zh(label: &str) -> &str {
    match label {
        "landscape" => "风景",
        "portrait" => "人像",
        "food" => "美食",
        "animal" => "动物",
        "architecture" => "建筑",
        "beach" => "海滩",
        "mountain" => "山",
        "sunset" => "日落",
        "night" => "夜景",
        "flower" => "花",
        "city" => "城市",
        "forest" => "森林",
        "snow" => "雪景",
        "water" => "水景",
        "car" => "汽车",
        "text" => "文字",
        "indoor" => "室内",
        "group photo" => "合照",
        "selfie" => "自拍",
        "screenshot" => "截图",
        "document" => "文档",
        "artwork" => "艺术品",
        "sport" => "运动",
        "concert" => "演唱会",
        "celebration" => "庆典",
        _ => label,
    }
}
