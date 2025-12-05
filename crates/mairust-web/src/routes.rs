//! Web UI Routes
//!
//! Defines routes for the web UI.

use crate::handlers;
use crate::AppState;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
};

/// Create the web UI router
pub fn create_router(state: AppState) -> Router {
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Static assets
        .route("/static/*path", get(handlers::static_file))
        // Page routes
        .route("/", get(handlers::index))
        .route("/inbox", get(handlers::inbox))
        .route("/compose", get(handlers::compose))
        .route("/message/:id", get(handlers::message))
        .route("/settings", get(handlers::settings))
        .route("/login", get(handlers::login_page))
        .route("/login", post(handlers::login_submit))
        .route("/logout", get(handlers::logout))
        // Health check
        .route("/health", get(handlers::health))
        // Add middleware
        .layer(CompressionLayer::new())
        .layer(cors)
        .with_state(state)
}
