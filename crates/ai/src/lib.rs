pub mod clip;
pub mod face;
pub mod processor;

use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

pub use clip::ClipModel;
pub use face::FaceModel;

/// Shared AI state accessible from the server
#[derive(Clone)]
pub struct AiState {
    pub clip: Arc<RwLock<Option<ClipModel>>>,
    pub face: Arc<RwLock<Option<FaceModel>>>,
}

impl AiState {
    pub fn new() -> Self {
        Self {
            clip: Arc::new(RwLock::new(None)),
            face: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize AI models from the given directory
    pub async fn init(&self, models_dir: &Path) -> anyhow::Result<()> {
        // Load CLIP models
        let vision_path = models_dir.join("clip-vision.onnx");
        let text_path = models_dir.join("clip-text.onnx");
        let tokenizer_path = models_dir.join("tokenizer.json");

        if !vision_path.exists() || !text_path.exists() || !tokenizer_path.exists() {
            tracing::warn!(
                "CLIP models not found in {:?}. Run models/download.sh to download.",
                models_dir
            );
        } else {
            tracing::info!("Loading CLIP models from {:?}...", models_dir);
            let model = ClipModel::load(&vision_path, &text_path, &tokenizer_path)?;
            tracing::info!("✅ CLIP models loaded successfully");
            let mut lock = self.clip.write().await;
            *lock = Some(model);
        }

        // Load face models
        let detect_path = models_dir.join("face-detect.onnx");
        let embed_path = models_dir.join("face-embed.onnx");

        if !detect_path.exists() || !embed_path.exists() {
            tracing::warn!(
                "Face models not found in {:?}. Run models/download_face.sh to download.",
                models_dir
            );
        } else {
            tracing::info!("Loading face models from {:?}...", models_dir);
            let model = FaceModel::load(&detect_path, &embed_path)?;
            tracing::info!("✅ Face models loaded successfully");
            let mut lock = self.face.write().await;
            *lock = Some(model);
        }

        Ok(())
    }
}
