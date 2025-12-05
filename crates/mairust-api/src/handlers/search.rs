//! Search handlers for full-text message search

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use mairust_core::search::{
    client::{MeilisearchClient, MeilisearchConfig as CoreMeilisearchConfig},
    indexer::{MessageIndexer, MessageSearchHit, SearchOptions},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// Search query parameters
#[derive(Debug, Clone, Deserialize)]
pub struct SearchQuery {
    /// Search query string
    pub q: String,
    /// Filter by mailbox ID
    pub mailbox_id: Option<Uuid>,
    /// Filter by sender address
    pub from: Option<String>,
    /// Filter by has attachments
    pub has_attachments: Option<bool>,
    /// Filter by read/seen status
    pub seen: Option<bool>,
    /// Filter by flagged status
    pub flagged: Option<bool>,
    /// Filter by tags (comma-separated)
    pub tags: Option<String>,
    /// Date range start (ISO 8601)
    pub date_from: Option<String>,
    /// Date range end (ISO 8601)
    pub date_to: Option<String>,
    /// Result offset
    pub offset: Option<u64>,
    /// Result limit (max 100)
    pub limit: Option<u64>,
}

/// Search response
#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub hits: Vec<MessageSearchHit>,
    pub query: String,
    pub processing_time_ms: u64,
    pub estimated_total_hits: Option<u64>,
    pub offset: u64,
    pub limit: u64,
}

/// Index status response
#[derive(Debug, Clone, Serialize)]
pub struct IndexStatusResponse {
    pub available: bool,
    pub message: String,
}

/// Get indexer from config (helper function)
fn create_indexer() -> Option<MessageIndexer> {
    // Read Meilisearch config from environment or default
    let url = std::env::var("MEILISEARCH_URL").unwrap_or_else(|_| "http://localhost:7700".to_string());
    let api_key = std::env::var("MEILISEARCH_API_KEY").ok();
    let messages_index =
        std::env::var("MEILISEARCH_INDEX").unwrap_or_else(|_| "messages".to_string());

    let enabled = std::env::var("MEILISEARCH_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if !enabled {
        return None;
    }

    let config = CoreMeilisearchConfig {
        url,
        api_key,
        timeout_secs: 30,
        messages_index,
    };

    Some(MessageIndexer::new(MeilisearchClient::new(config)))
}

/// Search messages within a tenant
pub async fn search_messages(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let indexer = create_indexer().ok_or_else(|| {
        warn!("Search not enabled");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    // Check if search service is available
    if !indexer.is_available().await {
        error!("Meilisearch service unavailable");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Parse date filters
    let date_from = query.date_from.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    let date_to = query.date_to.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    // Parse tags
    let tags = query.tags.map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

    // Limit the maximum results
    let limit = query.limit.map(|l| l.min(100)).unwrap_or(20);

    let options = SearchOptions {
        query: query.q.clone(),
        tenant_id,
        mailbox_id: query.mailbox_id,
        from_address: query.from,
        has_attachments: query.has_attachments,
        seen: query.seen,
        flagged: query.flagged,
        tags,
        date_from,
        date_to,
        offset: query.offset,
        limit: Some(limit),
    };

    let result = indexer.search(options).await.map_err(|e| {
        error!("Search error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!(
        "Search completed: query='{}', {} hits in {}ms",
        query.q,
        result.hits.len(),
        result.processing_time_ms
    );

    Ok(Json(SearchResponse {
        hits: result.hits,
        query: result.query,
        processing_time_ms: result.processing_time_ms,
        estimated_total_hits: result.estimated_total_hits,
        offset: query.offset.unwrap_or(0),
        limit,
    }))
}

/// Check search service status
pub async fn search_status(
    State(_state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<IndexStatusResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let indexer = match create_indexer() {
        Some(i) => i,
        None => {
            return Ok(Json(IndexStatusResponse {
                available: false,
                message: "Search is not enabled".to_string(),
            }));
        }
    };

    let available = indexer.is_available().await;

    Ok(Json(IndexStatusResponse {
        available,
        message: if available {
            "Search service is available".to_string()
        } else {
            "Search service is unavailable".to_string()
        },
    }))
}

/// Reindex a specific message (admin operation)
#[derive(Debug, Clone, Deserialize)]
pub struct ReindexRequest {
    pub message_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReindexResponse {
    pub task_uid: u64,
    pub message: String,
}

/// Request reindexing of messages
pub async fn reindex_messages(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(request): Json<ReindexRequest>,
) -> Result<Json<ReindexResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    if request.message_ids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let indexer = create_indexer().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    if !indexer.is_available().await {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // In a real implementation, we would fetch message data from the database
    // and index them. For now, we return a placeholder response.
    // This would be implemented by the caller by:
    // 1. Fetching messages from DB
    // 2. Converting to MessageDocument
    // 3. Calling indexer.index_messages()

    info!(
        "Reindex request for {} messages in tenant {}",
        request.message_ids.len(),
        tenant_id
    );

    Ok(Json(ReindexResponse {
        task_uid: 0,
        message: format!(
            "Reindex request received for {} messages. Implementation pending.",
            request.message_ids.len()
        ),
    }))
}
