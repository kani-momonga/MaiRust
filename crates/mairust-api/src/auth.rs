//! Authentication module

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use mairust_storage::DatabasePool;
use std::sync::Arc;
use tracing::debug;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db_pool: DatabasePool,
}

/// Extract API key from request
pub fn extract_api_key(req: &Request) -> Option<&str> {
    // Check Authorization header
    if let Some(auth) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Bearer ") {
                return Some(&auth_str[7..]);
            }
        }
    }

    // Check X-API-Key header
    if let Some(key) = req.headers().get("x-api-key") {
        if let Ok(key_str) = key.to_str() {
            return Some(key_str);
        }
    }

    None
}

/// Authentication middleware
pub async fn auth_middleware(
    State(_state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth for health check endpoints
    if request.uri().path().starts_with("/health") {
        return Ok(next.run(request).await);
    }

    // Extract API key
    let _api_key = extract_api_key(&request).ok_or(StatusCode::UNAUTHORIZED)?;

    // TODO: Validate API key against database
    debug!("API key authentication (validation not yet implemented)");

    Ok(next.run(request).await)
}
