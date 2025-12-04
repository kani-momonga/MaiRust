//! Health check handlers

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::auth::AppState;

/// Basic health response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Overall health status
    pub status: String,
}

/// Detailed health response with component checks
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DetailedHealthResponse {
    /// Overall health status
    pub status: String,
    /// Individual component health checks
    pub checks: HealthChecks,
}

/// Individual health checks
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthChecks {
    /// Database health status
    pub database: ComponentHealth,
}

/// Individual component health status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComponentHealth {
    /// Component status (healthy/unhealthy)
    pub status: String,
    /// Response latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Error message if unhealthy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Basic health check
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
    })
}

/// Liveness check (is the process running)
#[utoipa::path(
    get,
    path = "/health/live",
    tag = "health",
    responses(
        (status = 200, description = "Service is alive"),
        (status = 503, description = "Service is not alive")
    )
)]
pub async fn liveness() -> StatusCode {
    StatusCode::OK
}

/// Readiness check (is the service ready to accept requests)
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    responses(
        (status = 200, description = "Service is ready"),
        (status = 503, description = "Service is not ready")
    )
)]
pub async fn readiness(State(state): State<Arc<AppState>>) -> Result<StatusCode, StatusCode> {
    // Check database
    state
        .db_pool
        .health_check()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(StatusCode::OK)
}

/// Detailed health check with all dependencies
#[utoipa::path(
    get,
    path = "/health/detailed",
    tag = "health",
    responses(
        (status = 200, description = "Detailed health status", body = DetailedHealthResponse)
    )
)]
pub async fn health_detailed(State(state): State<Arc<AppState>>) -> Json<DetailedHealthResponse> {
    let start = std::time::Instant::now();
    let db_check = state.db_pool.health_check().await;
    let db_latency = start.elapsed().as_millis() as u64;

    let db_status = match db_check {
        Ok(_) => ComponentHealth {
            status: "healthy".to_string(),
            latency_ms: Some(db_latency),
            error: None,
        },
        Err(e) => ComponentHealth {
            status: "unhealthy".to_string(),
            latency_ms: None,
            error: Some(e.to_string()),
        },
    };

    let overall_status = if db_status.status == "healthy" {
        "healthy"
    } else {
        "unhealthy"
    };

    Json(DetailedHealthResponse {
        status: overall_status.to_string(),
        checks: HealthChecks {
            database: db_status,
        },
    })
}
