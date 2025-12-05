//! Admin Dashboard API handlers
//!
//! Provides statistics, usage reports, audit logs, and system configuration
//! for administrative dashboards.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::auth::{require_scope, require_tenant_access, AppState, AuthContext};

// ============================================================================
// System Statistics
// ============================================================================

/// System-wide statistics response
#[derive(Debug, Clone, Serialize)]
pub struct SystemStatsResponse {
    pub total_tenants: i64,
    pub active_tenants: i64,
    pub total_users: i64,
    pub total_mailboxes: i64,
    pub total_messages: i64,
    pub total_storage_bytes: i64,
    pub messages_last_24h: i64,
    pub messages_last_7d: i64,
    pub generated_at: DateTime<Utc>,
}

/// Get system-wide statistics (super admin only)
pub async fn get_system_stats(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<SystemStatsResponse>, StatusCode> {
    require_scope(&auth, "admin:system")?;

    // Query aggregate statistics
    let pool = state.db_pool.pool();

    let tenant_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tenants")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let active_tenant_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM tenants WHERE status = 'active'")
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mailbox_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM mailboxes")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let message_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let storage_sum: (Option<i64>,) =
        sqlx::query_as("SELECT COALESCE(SUM(body_size), 0) FROM messages")
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let now = Utc::now();
    let yesterday = now - Duration::hours(24);
    let last_week = now - Duration::days(7);

    let messages_24h: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM messages WHERE received_at >= $1")
            .bind(yesterday)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let messages_7d: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM messages WHERE received_at >= $1")
            .bind(last_week)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    Ok(Json(SystemStatsResponse {
        total_tenants: tenant_count.0,
        active_tenants: active_tenant_count.0,
        total_users: user_count.0,
        total_mailboxes: mailbox_count.0,
        total_messages: message_count.0,
        total_storage_bytes: storage_sum.0.unwrap_or(0),
        messages_last_24h: messages_24h.0,
        messages_last_7d: messages_7d.0,
        generated_at: now,
    }))
}

// ============================================================================
// Tenant Usage Report
// ============================================================================

/// Tenant usage statistics
#[derive(Debug, Clone, Serialize)]
pub struct TenantUsageResponse {
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub user_count: i64,
    pub domain_count: i64,
    pub mailbox_count: i64,
    pub message_count: i64,
    pub storage_bytes: i64,
    pub messages_last_24h: i64,
    pub messages_last_7d: i64,
    /// Usage limits from tenant plan
    pub limits: TenantLimits,
    pub generated_at: DateTime<Utc>,
}

/// Tenant resource limits
#[derive(Debug, Clone, Serialize, Default)]
pub struct TenantLimits {
    pub max_users: Option<i64>,
    pub max_domains: Option<i64>,
    pub max_storage_bytes: Option<i64>,
    pub max_daily_outbound: Option<i64>,
}

/// Get usage report for a specific tenant
pub async fn get_tenant_usage(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<TenantUsageResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let pool = state.db_pool.pool();

    // Get tenant info
    let tenant: Option<(String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT name, plan, settings FROM tenants WHERE id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (tenant_name, plan, settings) = tenant.ok_or(StatusCode::NOT_FOUND)?;

    // Get counts
    let user_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let domain_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM domains WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let mailbox_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM mailboxes WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let message_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM messages WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let storage_sum: (Option<i64>,) =
        sqlx::query_as("SELECT COALESCE(SUM(body_size), 0) FROM messages WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let now = Utc::now();
    let yesterday = now - Duration::hours(24);
    let last_week = now - Duration::days(7);

    let messages_24h: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1 AND received_at >= $2",
    )
    .bind(tenant_id)
    .bind(yesterday)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let messages_7d: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1 AND received_at >= $2",
    )
    .bind(tenant_id)
    .bind(last_week)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Parse limits from settings or use plan defaults
    let limits = parse_tenant_limits(&plan, &settings);

    Ok(Json(TenantUsageResponse {
        tenant_id,
        tenant_name,
        user_count: user_count.0,
        domain_count: domain_count.0,
        mailbox_count: mailbox_count.0,
        message_count: message_count.0,
        storage_bytes: storage_sum.0.unwrap_or(0),
        messages_last_24h: messages_24h.0,
        messages_last_7d: messages_7d.0,
        limits,
        generated_at: now,
    }))
}

fn parse_tenant_limits(plan: &str, settings: &serde_json::Value) -> TenantLimits {
    // Check settings first, then fall back to plan defaults
    let max_users = settings
        .get("max_users")
        .and_then(|v| v.as_i64())
        .or_else(|| match plan {
            "free" => Some(5),
            "pro" => Some(100),
            _ => None,
        });

    let max_domains = settings
        .get("max_domains")
        .and_then(|v| v.as_i64())
        .or_else(|| match plan {
            "free" => Some(1),
            "pro" => Some(10),
            _ => None,
        });

    let max_storage_bytes = settings
        .get("max_storage_gb")
        .and_then(|v| v.as_i64())
        .map(|gb| gb * 1024 * 1024 * 1024)
        .or_else(|| match plan {
            "free" => Some(1 * 1024 * 1024 * 1024),      // 1 GB
            "pro" => Some(50 * 1024 * 1024 * 1024),     // 50 GB
            _ => None,
        });

    let max_daily_outbound = settings
        .get("max_daily_outbound")
        .and_then(|v| v.as_i64())
        .or_else(|| match plan {
            "free" => Some(100),
            "pro" => Some(10000),
            _ => None,
        });

    TenantLimits {
        max_users,
        max_domains,
        max_storage_bytes,
        max_daily_outbound,
    }
}

// ============================================================================
// Audit Logs
// ============================================================================

/// Audit log entry
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for audit logs
#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogQuery {
    /// Filter by event type
    pub event_type: Option<String>,
    /// Filter by actor ID
    pub actor_id: Option<String>,
    /// Filter by target type
    pub target_type: Option<String>,
    /// Start date filter
    pub from: Option<String>,
    /// End date filter
    pub to: Option<String>,
    /// Pagination offset
    pub offset: Option<i64>,
    /// Pagination limit (max 100)
    pub limit: Option<i64>,
}

/// Audit log list response
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogListResponse {
    pub logs: Vec<AuditLogEntry>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// List audit logs for a tenant
pub async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<AuditLogListResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let pool = state.db_pool.pool();
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    // Build query with filters
    let mut sql = String::from(
        "SELECT id, tenant_id, actor_type, actor_id, event_type, target_type, target_id, details, ip_address, created_at
         FROM audit_logs WHERE tenant_id = $1"
    );
    let mut param_count = 1;

    if query.event_type.is_some() {
        param_count += 1;
        sql.push_str(&format!(" AND event_type = ${}", param_count));
    }

    if query.actor_id.is_some() {
        param_count += 1;
        sql.push_str(&format!(" AND actor_id = ${}", param_count));
    }

    if query.target_type.is_some() {
        param_count += 1;
        sql.push_str(&format!(" AND target_type = ${}", param_count));
    }

    sql.push_str(" ORDER BY created_at DESC LIMIT $");
    param_count += 1;
    sql.push_str(&param_count.to_string());
    sql.push_str(" OFFSET $");
    param_count += 1;
    sql.push_str(&param_count.to_string());

    // For now, use simpler query
    let logs: Vec<AuditLogEntry> = sqlx::query_as(
        "SELECT id, tenant_id, actor_type, actor_id, event_type, target_type, target_id, details, ip_address, created_at
         FROM audit_logs WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
    )
    .bind(tenant_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_logs WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(AuditLogListResponse {
        logs,
        total: total.0,
        offset,
        limit,
    }))
}

// ============================================================================
// All Tenants Usage (Super Admin)
// ============================================================================

/// Tenant summary for admin list
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TenantSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan: String,
    pub created_at: DateTime<Utc>,
}

/// List all tenants with summary (super admin only)
pub async fn list_all_tenants_summary(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<TenantListResponse>, StatusCode> {
    require_scope(&auth, "admin:system")?;

    let pool = state.db_pool.pool();
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let tenants: Vec<TenantSummary> = sqlx::query_as(
        "SELECT id, name, slug, status, plan, created_at FROM tenants ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tenants")
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(TenantListResponse {
        tenants,
        total: total.0,
        offset,
        limit,
    }))
}

/// Pagination query parameters
#[derive(Debug, Clone, Deserialize)]
pub struct PaginationQuery {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// Tenant list response
#[derive(Debug, Clone, Serialize)]
pub struct TenantListResponse {
    pub tenants: Vec<TenantSummary>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

