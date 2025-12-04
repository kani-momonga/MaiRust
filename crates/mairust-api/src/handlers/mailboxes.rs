//! Mailbox handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use mairust_storage::{CreateMailbox, Mailbox, MailboxRepository, MailboxRepositoryTrait};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AppState;

/// Query parameters for listing mailboxes
#[derive(Debug, Clone, Deserialize)]
pub struct ListMailboxesQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub domain_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

fn default_limit() -> i64 {
    50
}

/// Request body for creating a mailbox
#[derive(Debug, Clone, Deserialize)]
pub struct CreateMailboxRequest {
    pub domain_id: Uuid,
    pub user_id: Option<Uuid>,
    pub address: String,
    pub display_name: Option<String>,
    pub quota_bytes: Option<i64>,
}

/// Request body for updating mailbox quota
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateQuotaRequest {
    pub quota_bytes: Option<i64>,
}

/// Mailbox with usage stats
#[derive(Debug, Clone, Serialize)]
pub struct MailboxResponse {
    #[serde(flatten)]
    pub mailbox: Mailbox,
    pub usage_percent: Option<f64>,
}

impl From<Mailbox> for MailboxResponse {
    fn from(mailbox: Mailbox) -> Self {
        let usage_percent = mailbox.quota_bytes.map(|quota| {
            if quota > 0 {
                (mailbox.used_bytes as f64 / quota as f64) * 100.0
            } else {
                0.0
            }
        });

        Self {
            mailbox,
            usage_percent,
        }
    }
}

/// List mailboxes for a tenant
pub async fn list_mailboxes(
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<ListMailboxesQuery>,
) -> Result<Json<Vec<MailboxResponse>>, StatusCode> {
    let repo = MailboxRepository::new(state.db_pool.clone());

    let mailboxes = if let Some(domain_id) = query.domain_id {
        repo.list_by_domain(domain_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else if let Some(user_id) = query.user_id {
        repo.list_by_user(user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        repo.list(tenant_id, query.limit, query.offset)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let responses: Vec<MailboxResponse> = mailboxes.into_iter().map(Into::into).collect();

    Ok(Json(responses))
}

/// Get a mailbox by ID
pub async fn get_mailbox(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, mailbox_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<MailboxResponse>, StatusCode> {
    let repo = MailboxRepository::new(state.db_pool.clone());

    let mailbox = repo
        .get(tenant_id, mailbox_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(mailbox.into()))
}

/// Create a new mailbox
pub async fn create_mailbox(
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreateMailboxRequest>,
) -> Result<(StatusCode, Json<MailboxResponse>), StatusCode> {
    let repo = MailboxRepository::new(state.db_pool.clone());

    // Check if address already exists
    if let Ok(Some(_)) = repo.get_by_address(&input.address).await {
        return Err(StatusCode::CONFLICT);
    }

    let create_input = CreateMailbox {
        tenant_id,
        domain_id: input.domain_id,
        user_id: input.user_id,
        address: input.address.to_lowercase(),
        display_name: input.display_name,
        quota_bytes: input.quota_bytes,
    };

    let mailbox = repo
        .create(create_input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(mailbox.into())))
}

/// Update mailbox quota
pub async fn update_mailbox_quota(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, mailbox_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateQuotaRequest>,
) -> Result<Json<MailboxResponse>, StatusCode> {
    let repo = MailboxRepository::new(state.db_pool.clone());

    // Check mailbox exists and belongs to tenant
    let _ = repo
        .get(tenant_id, mailbox_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.update_quota(mailbox_id, input.quota_bytes)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mailbox = repo
        .get(tenant_id, mailbox_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(mailbox.into()))
}

/// Delete a mailbox
pub async fn delete_mailbox(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, mailbox_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let repo = MailboxRepository::new(state.db_pool.clone());

    // Check mailbox exists and belongs to tenant
    let _ = repo
        .get(tenant_id, mailbox_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.delete(mailbox_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
