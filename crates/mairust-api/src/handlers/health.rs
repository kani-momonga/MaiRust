//! Health check handlers

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::AppState;

/// Health status
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub checks: HealthChecks,
}

/// Individual health checks
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthChecks {
    pub database: CheckStatus,
}

/// Individual check status
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Basic health check
pub async fn health() -> Json<HealthStatus> {
    Json(HealthStatus {
        status: "healthy".to_string(),
        checks: HealthChecks {
            database: CheckStatus {
                status: "unknown".to_string(),
                latency_ms: None,
                error: None,
            },
        },
    })
}

/// Liveness check (is the process running)
pub async fn liveness() -> StatusCode {
    StatusCode::OK
}

/// Readiness check (is the service ready to accept requests)
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
pub async fn health_detailed(State(state): State<Arc<AppState>>) -> Json<HealthStatus> {
    let start = std::time::Instant::now();
    let db_check = state.db_pool.health_check().await;
    let db_latency = start.elapsed().as_millis() as u64;

    let db_status = match db_check {
        Ok(_) => CheckStatus {
            status: "healthy".to_string(),
            latency_ms: Some(db_latency),
            error: None,
        },
        Err(e) => CheckStatus {
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

    Json(HealthStatus {
        status: overall_status.to_string(),
        checks: HealthChecks {
            database: db_status,
        },
    })
}
