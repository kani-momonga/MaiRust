//! Message handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use mairust_storage::{Message, MessageRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AppState;

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
    Query(query): Query<ListMessagesQuery>,
) -> Result<Json<MessageListResponse>, StatusCode> {
    let repo = MessageRepository::new(state.db_pool.clone());
    let limit = query.limit.unwrap_or(50).min(100);

    // TODO: Implement cursor-based pagination
    let messages = repo
        .find_by_mailbox(query.mailbox_id, limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
    Path(message_id): Path<Uuid>,
) -> Result<Json<Message>, StatusCode> {
    let repo = MessageRepository::new(state.db_pool.clone());

    let message = repo
        .find_by_id(message_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

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
    Path(message_id): Path<Uuid>,
    Json(req): Json<UpdateFlagsRequest>,
) -> Result<StatusCode, StatusCode> {
    let repo = MessageRepository::new(state.db_pool.clone());

    repo.update_flags(
        message_id,
        req.seen,
        req.answered,
        req.flagged,
        req.deleted,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Delete a message
pub async fn delete_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let repo = MessageRepository::new(state.db_pool.clone());

    repo.delete(message_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
