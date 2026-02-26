mod auth;
mod error;
mod routes;
mod state;

use axum::Router;
use fast_photo_ai::AiState;
use fast_photo_common::AppConfig;
use fast_photo_core::db;
use fast_photo_core::models::ScanProgress;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fast_photo=info,tower_http=info".into()),
        )
        .init();

    tracing::info!("🚀 FastPhoto starting...");

    // Load configuration
    let config = AppConfig::load("config.toml")?;
    tracing::info!(
        "Config loaded: {}:{}",
        config.server.host,
        config.server.port
    );

    // Ensure storage directories exist
    std::fs::create_dir_all(&config.storage.data_dir)?;
    std::fs::create_dir_all(&config.storage.thumbnail_dir)?;

    // Initialize database
    let pool = db::init_pool(&config.database.url).await?;
    tracing::info!("Database initialized");

    // Initialize AI
    let ai_state = AiState::new();
    let models_dir = std::path::Path::new("models");
    if let Err(e) = ai_state.init(models_dir).await {
        tracing::warn!(
            "Failed to initialize AI models: {}. AI features will be unavailable.",
            e
        );
    }

    // Scan progress channel
    let (scan_tx, scan_rx) = watch::channel(ScanProgress {
        library_id: 0,
        total_files: 0,
        processed_files: 0,
        status: "idle".to_string(),
    });

    let state = AppState {
        db: Arc::new(pool),
        config: Arc::new(config.clone()),
        scan_progress: scan_rx,
        scan_tx: Arc::new(scan_tx),
        ai: ai_state,
    };

    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the router
    let app = Router::new()
        .nest("/api", routes::api_routes())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr = SocketAddr::new(config.server.host.parse()?, config.server.port);
    tracing::info!("🌐 Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
