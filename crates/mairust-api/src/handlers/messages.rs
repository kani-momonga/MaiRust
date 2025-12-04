//! Message handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use mairust_storage::repository::messages::MessageRepository as MessageRepositoryTrait;
use mairust_storage::{MailboxRepository, MailboxRepositoryTrait, Message, MessageRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, warn};
use uuid::Uuid;

use crate::auth::{AppState, AuthContext};

/// List messages query parameters
#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    pub mailbox_id: Uuid,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

/// Message list response
#[derive(Debug, Serialize)]
pub struct MessageListResponse {
    pub data: Vec<MessageSummary>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

/// Message summary (for list view)
#[derive(Debug, Serialize)]
pub struct MessageSummary {
    pub id: Uuid,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub received_at: chrono::DateTime<chrono::Utc>,
    pub seen: bool,
    pub flagged: bool,
    pub has_attachments: bool,
}

impl From<Message> for MessageSummary {
    fn from(msg: Message) -> Self {
        Self {
            id: msg.id,
            subject: msg.subject,
            from_address: msg.from_address,
            received_at: msg.received_at,
            seen: msg.seen,
            flagged: msg.flagged,
            has_attachments: msg.has_attachments,
        }
    }
}

/// List messages in a mailbox
pub async fn list_messages(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<ListMessagesQuery>,
) -> Result<Json<MessageListResponse>, StatusCode> {
    // Verify mailbox belongs to the authenticated tenant
    let mailbox_repo = MailboxRepository::new(state.db_pool.clone());
    let mailbox = mailbox_repo
        .get(auth.tenant_id, query.mailbox_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching mailbox: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Mailbox {} not found or not owned by tenant {}",
                query.mailbox_id, auth.tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    let repo = MessageRepository::new(state.db_pool.clone());
    let limit = query.limit.unwrap_or(50).min(100);

    let messages = MessageRepositoryTrait::list(&repo, auth.tenant_id, mailbox.id, limit as i64, 0)
        .await
        .map_err(|e| {
            error!("Database error while listing messages: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let summaries: Vec<MessageSummary> = messages.into_iter().map(Into::into).collect();
    let has_more = summaries.len() >= limit;

    Ok(Json(MessageListResponse {
        data: summaries,
        cursor: None, // TODO: Implement cursor
        has_more,
    }))
}

/// Get a single message
pub async fn get_message(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(message_id): Path<Uuid>,
) -> Result<Json<Message>, StatusCode> {
    let repo = MessageRepository::new(state.db_pool.clone());

    // Use tenant-aware get method to ensure tenant isolation
    let message = MessageRepositoryTrait::get(&repo, auth.tenant_id, message_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching message: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Message {} not found or not owned by tenant {}",
                message_id, auth.tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    Ok(Json(message))
}

/// Update message flags
#[derive(Debug, Deserialize)]
pub struct UpdateFlagsRequest {
    pub seen: Option<bool>,
    pub flagged: Option<bool>,
    pub answered: Option<bool>,
    pub deleted: Option<bool>,
}

pub async fn update_message_flags(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(message_id): Path<Uuid>,
    Json(req): Json<UpdateFlagsRequest>,
) -> Result<StatusCode, StatusCode> {
    let repo = MessageRepository::new(state.db_pool.clone());

    // Verify message belongs to tenant before updating
    let _ = MessageRepositoryTrait::get(&repo, auth.tenant_id, message_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching message: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Message {} not found or not owned by tenant {}",
                message_id, auth.tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    // Use tenant-aware update method via trait
    MessageRepositoryTrait::update_flags(
        &repo,
        auth.tenant_id,
        message_id,
        req.seen,
        req.flagged,
        req.answered,
        req.deleted,
    )
    .await
    .map_err(|e| {
        error!("Database error while updating message flags: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Delete a message
pub async fn delete_message(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(message_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let repo = MessageRepository::new(state.db_pool.clone());

    // Verify message belongs to tenant before deleting
    let _ = MessageRepositoryTrait::get(&repo, auth.tenant_id, message_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching message: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Message {} not found or not owned by tenant {}",
                message_id, auth.tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    // Use tenant-aware delete method via trait
    MessageRepositoryTrait::delete(&repo, auth.tenant_id, message_id)
        .await
        .map_err(|e| {
            error!("Database error while deleting message: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}
