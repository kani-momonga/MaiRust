//! Recipient list handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use mairust_storage::models::{
    CreateRecipient, CreateRecipientList, Recipient, RecipientList, RecipientStatus,
    UpdateRecipient, UpdateRecipientList,
};
use mairust_storage::repository::{RecipientListRepository, RecipientRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// Error response
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

/// Query parameters for listing
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub status: Option<String>,
}

fn default_limit() -> i64 {
    50
}

/// Recipient list response
#[derive(Debug, Serialize)]
pub struct RecipientListResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub recipient_count: i32,
    pub active_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<RecipientList> for RecipientListResponse {
    fn from(r: RecipientList) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            recipient_count: r.recipient_count,
            active_count: r.active_count,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// List response
#[derive(Debug, Serialize)]
pub struct RecipientListsResponse {
    pub data: Vec<RecipientListResponse>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Recipient response
#[derive(Debug, Serialize)]
pub struct RecipientResponse {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub status: String,
    pub attributes: serde_json::Value,
    pub subscribed_at: DateTime<Utc>,
    pub unsubscribed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<Recipient> for RecipientResponse {
    fn from(r: Recipient) -> Self {
        Self {
            id: r.id,
            email: r.email,
            name: r.name,
            status: r.status,
            attributes: r.attributes,
            subscribed_at: r.subscribed_at,
            unsubscribed_at: r.unsubscribed_at,
            created_at: r.created_at,
        }
    }
}

/// Recipients list response
#[derive(Debug, Serialize)]
pub struct RecipientsResponse {
    pub data: Vec<RecipientResponse>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Request body for creating a recipient list
#[derive(Debug, Deserialize)]
pub struct CreateRecipientListRequest {
    pub name: String,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Request body for updating a recipient list
#[derive(Debug, Deserialize)]
pub struct UpdateRecipientListRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Request body for adding a recipient
#[derive(Debug, Deserialize)]
pub struct AddRecipientRequest {
    pub email: String,
    pub name: Option<String>,
    pub attributes: Option<serde_json::Value>,
}

/// Request body for updating a recipient
#[derive(Debug, Deserialize)]
pub struct UpdateRecipientRequest {
    pub name: Option<String>,
    pub status: Option<String>,
    pub attributes: Option<serde_json::Value>,
}

/// Request body for importing recipients
#[derive(Debug, Deserialize)]
pub struct ImportRecipientsRequest {
    pub recipients: Vec<ImportRecipient>,
}

#[derive(Debug, Deserialize)]
pub struct ImportRecipient {
    pub email: String,
    pub name: Option<String>,
    pub attributes: Option<serde_json::Value>,
}

/// Import result
#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub imported: u64,
    pub skipped: u64,
    pub total: usize,
}

// =============================================================================
// Recipient List Handlers
// =============================================================================

/// List recipient lists
///
/// GET /api/v1/tenants/:tenant_id/recipient-lists
pub async fn list_recipient_lists(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> Result<Json<RecipientListsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let repo = RecipientListRepository::new(state.db_pool.pool().clone());

    let lists = repo
        .list_by_tenant(tenant_id, query.limit, query.offset)
        .await
        .map_err(|e| {
            error!("Failed to list recipient lists: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to list recipient lists".to_string(),
                }),
            )
        })?;

    let total = repo.count_by_tenant(tenant_id).await.unwrap_or(0);

    let data = lists.into_iter().map(RecipientListResponse::from).collect();

    Ok(Json(RecipientListsResponse {
        data,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

/// Create a recipient list
///
/// POST /api/v1/tenants/:tenant_id/recipient-lists
pub async fn create_recipient_list(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreateRecipientListRequest>,
) -> Result<(StatusCode, Json<RecipientListResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    if input.name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Name is required".to_string(),
            }),
        ));
    }

    let repo = RecipientListRepository::new(state.db_pool.pool().clone());

    let create_input = CreateRecipientList {
        tenant_id,
        name: input.name,
        description: input.description,
        metadata: input.metadata,
    };

    let list = repo.create(create_input).await.map_err(|e| {
        error!("Failed to create recipient list: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to create recipient list".to_string(),
            }),
        )
    })?;

    info!("Created recipient list {} for tenant {}", list.id, tenant_id);

    Ok((StatusCode::CREATED, Json(RecipientListResponse::from(list))))
}

/// Get a recipient list
///
/// GET /api/v1/tenants/:tenant_id/recipient-lists/:list_id
pub async fn get_recipient_list(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<RecipientListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let repo = RecipientListRepository::new(state.db_pool.pool().clone());

    let list = repo
        .get_by_tenant(tenant_id, list_id)
        .await
        .map_err(|e| {
            error!("Failed to get recipient list: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to get recipient list".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient list not found".to_string(),
                }),
            )
        })?;

    Ok(Json(RecipientListResponse::from(list)))
}

/// Update a recipient list
///
/// PUT /api/v1/tenants/:tenant_id/recipient-lists/:list_id
pub async fn update_recipient_list(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateRecipientListRequest>,
) -> Result<Json<RecipientListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let repo = RecipientListRepository::new(state.db_pool.pool().clone());

    let update_input = UpdateRecipientList {
        name: input.name,
        description: input.description,
        metadata: input.metadata,
    };

    let list = repo
        .update(list_id, tenant_id, update_input)
        .await
        .map_err(|e| {
            error!("Failed to update recipient list: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to update recipient list".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient list not found".to_string(),
                }),
            )
        })?;

    info!("Updated recipient list {}", list_id);

    Ok(Json(RecipientListResponse::from(list)))
}

/// Delete a recipient list
///
/// DELETE /api/v1/tenants/:tenant_id/recipient-lists/:list_id
pub async fn delete_recipient_list(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let repo = RecipientListRepository::new(state.db_pool.pool().clone());

    let deleted = repo.delete(list_id, tenant_id).await.map_err(|e| {
        error!("Failed to delete recipient list: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to delete recipient list".to_string(),
            }),
        )
    })?;

    if deleted {
        info!("Deleted recipient list {}", list_id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "Recipient list not found".to_string(),
            }),
        ))
    }
}

// =============================================================================
// Recipient Handlers
// =============================================================================

/// List recipients in a list
///
/// GET /api/v1/tenants/:tenant_id/recipient-lists/:list_id/recipients
pub async fn list_recipients(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<ListQuery>,
) -> Result<Json<RecipientsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    // Verify list belongs to tenant
    let list_repo = RecipientListRepository::new(state.db_pool.pool().clone());
    list_repo
        .get_by_tenant(tenant_id, list_id)
        .await
        .map_err(|e| {
            error!("Failed to verify recipient list: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to verify recipient list".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient list not found".to_string(),
                }),
            )
        })?;

    let repo = RecipientRepository::new(state.db_pool.pool().clone());

    let status = query
        .status
        .and_then(|s| s.parse::<RecipientStatus>().ok());

    let recipients = repo
        .list_by_list(list_id, status, query.limit, query.offset)
        .await
        .map_err(|e| {
            error!("Failed to list recipients: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to list recipients".to_string(),
                }),
            )
        })?;

    let total = repo.count_by_list(list_id, status).await.unwrap_or(0);

    let data = recipients.into_iter().map(RecipientResponse::from).collect();

    Ok(Json(RecipientsResponse {
        data,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

/// Add a recipient to a list
///
/// POST /api/v1/tenants/:tenant_id/recipient-lists/:list_id/recipients
pub async fn add_recipient(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<AddRecipientRequest>,
) -> Result<(StatusCode, Json<RecipientResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    // Validate email
    if !input.email.contains('@') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Invalid email address".to_string(),
            }),
        ));
    }

    // Verify list belongs to tenant
    let list_repo = RecipientListRepository::new(state.db_pool.pool().clone());
    list_repo
        .get_by_tenant(tenant_id, list_id)
        .await
        .map_err(|e| {
            error!("Failed to verify recipient list: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to verify recipient list".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient list not found".to_string(),
                }),
            )
        })?;

    let repo = RecipientRepository::new(state.db_pool.pool().clone());

    let create_input = CreateRecipient {
        recipient_list_id: list_id,
        email: input.email.to_lowercase(),
        name: input.name,
        attributes: input.attributes,
    };

    let recipient = repo.create(create_input).await.map_err(|e| {
        let message = if e.to_string().contains("unique") {
            "Email already exists in this list"
        } else {
            "Failed to add recipient"
        };
        error!("Failed to add recipient: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "add_error".to_string(),
                message: message.to_string(),
            }),
        )
    })?;

    info!("Added recipient {} to list {}", recipient.id, list_id);

    Ok((StatusCode::CREATED, Json(RecipientResponse::from(recipient))))
}

/// Get a recipient
///
/// GET /api/v1/tenants/:tenant_id/recipient-lists/:list_id/recipients/:recipient_id
pub async fn get_recipient(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id, recipient_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<RecipientResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    // Verify list belongs to tenant
    let list_repo = RecipientListRepository::new(state.db_pool.pool().clone());
    list_repo
        .get_by_tenant(tenant_id, list_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to verify recipient list".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient list not found".to_string(),
                }),
            )
        })?;

    let repo = RecipientRepository::new(state.db_pool.pool().clone());

    let recipient = repo.get(recipient_id).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to get recipient".to_string(),
            }),
        )
    })?;

    match recipient {
        Some(r) if r.recipient_list_id == list_id => Ok(Json(RecipientResponse::from(r))),
        _ => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "Recipient not found".to_string(),
            }),
        )),
    }
}

/// Update a recipient
///
/// PUT /api/v1/tenants/:tenant_id/recipient-lists/:list_id/recipients/:recipient_id
pub async fn update_recipient(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id, recipient_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(input): Json<UpdateRecipientRequest>,
) -> Result<Json<RecipientResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    // Verify list belongs to tenant
    let list_repo = RecipientListRepository::new(state.db_pool.pool().clone());
    list_repo
        .get_by_tenant(tenant_id, list_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to verify recipient list".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient list not found".to_string(),
                }),
            )
        })?;

    let repo = RecipientRepository::new(state.db_pool.pool().clone());

    // Verify recipient belongs to list
    let existing = repo.get(recipient_id).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to get recipient".to_string(),
            }),
        )
    })?;

    if existing.is_none() || existing.as_ref().map(|r| r.recipient_list_id) != Some(list_id) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "Recipient not found".to_string(),
            }),
        ));
    }

    let status = input
        .status
        .and_then(|s| s.parse::<RecipientStatus>().ok());

    let update_input = UpdateRecipient {
        name: input.name,
        status,
        attributes: input.attributes,
    };

    let recipient = repo
        .update(recipient_id, update_input)
        .await
        .map_err(|e| {
            error!("Failed to update recipient: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to update recipient".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient not found".to_string(),
                }),
            )
        })?;

    info!("Updated recipient {}", recipient_id);

    Ok(Json(RecipientResponse::from(recipient)))
}

/// Delete a recipient
///
/// DELETE /api/v1/tenants/:tenant_id/recipient-lists/:list_id/recipients/:recipient_id
pub async fn delete_recipient(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id, recipient_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    // Verify list belongs to tenant
    let list_repo = RecipientListRepository::new(state.db_pool.pool().clone());
    list_repo
        .get_by_tenant(tenant_id, list_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to verify recipient list".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient list not found".to_string(),
                }),
            )
        })?;

    let repo = RecipientRepository::new(state.db_pool.pool().clone());

    // Verify recipient belongs to list
    let existing = repo.get(recipient_id).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to get recipient".to_string(),
            }),
        )
    })?;

    if existing.is_none() || existing.as_ref().map(|r| r.recipient_list_id) != Some(list_id) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "Recipient not found".to_string(),
            }),
        ));
    }

    let deleted = repo.delete(recipient_id).await.map_err(|e| {
        error!("Failed to delete recipient: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to delete recipient".to_string(),
            }),
        )
    })?;

    if deleted {
        info!("Deleted recipient {}", recipient_id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "Recipient not found".to_string(),
            }),
        ))
    }
}

/// Import recipients in batch
///
/// POST /api/v1/tenants/:tenant_id/recipient-lists/:list_id/import
pub async fn import_recipients(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, list_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<ImportRecipientsRequest>,
) -> Result<Json<ImportResult>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    if input.recipients.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "No recipients provided".to_string(),
            }),
        ));
    }

    // Verify list belongs to tenant
    let list_repo = RecipientListRepository::new(state.db_pool.pool().clone());
    list_repo
        .get_by_tenant(tenant_id, list_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to verify recipient list".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Recipient list not found".to_string(),
                }),
            )
        })?;

    let repo = RecipientRepository::new(state.db_pool.pool().clone());

    let total = input.recipients.len();

    // Prepare batch data
    let recipients: Vec<(String, Option<String>, Option<serde_json::Value>)> = input
        .recipients
        .into_iter()
        .filter(|r| r.email.contains('@'))
        .map(|r| (r.email.to_lowercase(), r.name, r.attributes))
        .collect();

    let skipped = total - recipients.len();

    let imported = repo
        .create_batch(list_id, recipients)
        .await
        .map_err(|e| {
            error!("Failed to import recipients: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "import_error".to_string(),
                    message: "Failed to import recipients".to_string(),
                }),
            )
        })?;

    info!(
        "Imported {} recipients to list {} ({} skipped)",
        imported, list_id, skipped
    );

    Ok(Json(ImportResult {
        imported,
        skipped: skipped as u64,
        total,
    }))
}
