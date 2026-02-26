use fast_photo_ai::AiState;
use fast_photo_common::AppConfig;
use fast_photo_core::db::DbPool;
use fast_photo_core::models::ScanProgress;
use std::sync::Arc;
use tokio::sync::watch;

use crate::alerts::AlertSink;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DbPool>,
    pub config: Arc<AppConfig>,
    pub scan_progress: watch::Receiver<ScanProgress>,
    pub scan_tx: Arc<watch::Sender<ScanProgress>>,
    pub ai: AiState,
    pub alerts: Arc<AlertSink>,
}
