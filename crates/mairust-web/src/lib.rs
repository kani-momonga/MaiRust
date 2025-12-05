//! MaiRust Web UI
//!
//! Web-based email client interface for MaiRust.

mod handlers;
mod routes;
mod templates;

use axum::Router;
use mairust_storage::db::DatabasePool;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Static files for the web UI
#[derive(RustEmbed)]
#[folder = "static/"]
pub struct StaticAssets;

/// Web UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Listen address
    #[serde(default = "default_bind")]
    pub bind: String,
    /// API base URL (for frontend API calls)
    #[serde(default = "default_api_url")]
    pub api_url: String,
    /// Enable debug mode
    #[serde(default)]
    pub debug: bool,
}

fn default_bind() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_api_url() -> String {
    "/api/v1".to_string()
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            api_url: default_api_url(),
            debug: false,
        }
    }
}

/// Application state for the web UI
#[derive(Clone)]
pub struct AppState {
    pub config: WebConfig,
    pub db_pool: DatabasePool,
    pub templates: Arc<templates::Templates>,
}

impl AppState {
    /// Create a new app state
    pub fn new(config: WebConfig, db_pool: DatabasePool) -> Self {
        Self {
            config,
            db_pool,
            templates: Arc::new(templates::Templates::new()),
        }
    }
}

/// Create the web UI router
pub fn create_router(state: AppState) -> Router {
    routes::create_router(state)
}

/// Run the web UI server
pub async fn run(config: WebConfig, db_pool: DatabasePool) -> anyhow::Result<()> {
    let state = AppState::new(config.clone(), db_pool);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!("Web UI listening on {}", config.bind);

    axum::serve(listener, app).await?;

    Ok(())
}
