use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize)]
pub struct AlertEvent {
    pub timestamp: chrono::DateTime<Utc>,
    pub level: String,
    pub kind: String,
    pub message: String,
    pub request_id: Option<String>,
    pub method: Option<String>,
    pub path: Option<String>,
    pub status: Option<u16>,
    pub latency_ms: Option<u64>,
}

impl AlertEvent {
    pub fn http_5xx(
        request_id: String,
        method: String,
        path: String,
        status: u16,
        latency_ms: u64,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            level: "error".to_string(),
            kind: "http_5xx".to_string(),
            message: "HTTP 5xx response detected".to_string(),
            request_id: Some(request_id),
            method: Some(method),
            path: Some(path),
            status: Some(status),
            latency_ms: Some(latency_ms),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlertSink {
    output_path: Arc<PathBuf>,
}

impl AlertSink {
    pub fn new(output_path: PathBuf) -> Self {
        Self {
            output_path: Arc::new(output_path),
        }
    }

    pub fn output_path(&self) -> PathBuf {
        self.output_path.as_ref().clone()
    }

    pub async fn emit(&self, event: &AlertEvent) {
        if let Err(e) = self.try_emit(event).await {
            tracing::warn!("Failed to write alert event: {}", e);
        }
    }

    async fn try_emit(&self, event: &AlertEvent) -> anyhow::Result<()> {
        if let Some(parent) = self.output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let serialized = serde_json::to_string(event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.output_path.as_ref())
            .await?;
        file.write_all(serialized.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn alert_sink_writes_ndjson_line() {
        let path = std::env::temp_dir().join(format!("fast-photo-alerts-{}.ndjson", Uuid::new_v4()));
        let sink = AlertSink::new(path.clone());
        let event = AlertEvent::http_5xx(
            "req-1".to_string(),
            "GET".to_string(),
            "/api/test".to_string(),
            500,
            12,
        );

        sink.emit(&event).await;
        let content = tokio::fs::read_to_string(&path).await.expect("read alert file");
        assert!(content.contains("\"kind\":\"http_5xx\""));
        assert!(content.contains("\"request_id\":\"req-1\""));

        let _ = tokio::fs::remove_file(path).await;
    }
}
