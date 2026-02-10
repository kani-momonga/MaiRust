//! Send email handler

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use base64::Engine;
use chrono::Utc;
use mairust_storage::{DatabasePool, MailboxRepository, MailboxRepositoryTrait};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, warn};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// Maximum total message size (10MB)
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

/// Maximum single attachment size (5MB)
const MAX_ATTACHMENT_SIZE: usize = 5 * 1024 * 1024;

/// Maximum number of attachments
const MAX_ATTACHMENTS: usize = 10;

/// Maximum number of recipients
const MAX_RECIPIENTS: usize = 100;

/// Email attachment
#[derive(Debug, Clone, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    /// Base64 encoded content
    pub content: String,
}

/// Request body for sending an email
#[derive(Debug, Clone, Deserialize)]
pub struct SendEmailRequest {
    /// Sender email address (must be a verified mailbox)
    pub from: String,
    /// List of recipient email addresses
    pub to: Vec<String>,
    /// Carbon copy recipients
    #[serde(default)]
    pub cc: Vec<String>,
    /// Blind carbon copy recipients
    #[serde(default)]
    pub bcc: Vec<String>,
    /// Email subject
    pub subject: Option<String>,
    /// Plain text body
    pub text: Option<String>,
    /// HTML body
    pub html: Option<String>,
    /// Custom headers
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    /// Attachments
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Reply-To address
    pub reply_to: Option<String>,
    /// Schedule send time (ISO 8601)
    pub scheduled_at: Option<chrono::DateTime<Utc>>,
    /// Custom Message-ID (generated if not provided)
    pub message_id: Option<String>,
}

/// Response after queuing an email
#[derive(Debug, Clone, Serialize)]
pub struct SendEmailResponse {
    /// Unique message ID
    pub message_id: Uuid,
    /// Status of the send request
    pub status: String,
    /// Estimated recipients
    pub recipients_count: usize,
    /// Scheduled send time
    pub scheduled_at: Option<chrono::DateTime<Utc>>,
    /// Queue position (if queued)
    pub queue_id: Option<Uuid>,
}

/// Validate email address format
fn is_valid_email(email: &str) -> bool {
    // Basic validation: contains @ and has domain part
    if let Some(at_pos) = email.rfind('@') {
        let domain = &email[at_pos + 1..];
        !email[..at_pos].is_empty() && !domain.is_empty() && domain.contains('.')
    } else {
        false
    }
}

/// Extract domain from email address
fn extract_domain(email: &str) -> Option<&str> {
    email.rfind('@').map(|pos| &email[pos + 1..])
}

/// Send an email
///
/// POST /api/v1/tenants/:tenant_id/send
///
/// This endpoint accepts an email, validates it, and queues it for delivery.
pub async fn send_email(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<SendEmailRequest>,
) -> Result<(StatusCode, Json<SendEmailResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    // Validate input
    if input.to.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "At least one recipient is required".to_string(),
            }),
        ));
    }

    // Validate recipient count
    let total_recipients = input.to.len() + input.cc.len() + input.bcc.len();
    if total_recipients > MAX_RECIPIENTS {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: format!(
                    "Too many recipients: {} (max {})",
                    total_recipients, MAX_RECIPIENTS
                ),
            }),
        ));
    }

    // Validate attachment count
    if input.attachments.len() > MAX_ATTACHMENTS {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: format!(
                    "Too many attachments: {} (max {})",
                    input.attachments.len(),
                    MAX_ATTACHMENTS
                ),
            }),
        ));
    }

    // Validate attachments
    for (i, attachment) in input.attachments.iter().enumerate() {
        // Validate base64 content
        if let Err(e) = validate_attachment(attachment) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message: format!("Invalid attachment {}: {}", i + 1, e),
                }),
            ));
        }
    }

    if input.text.is_none() && input.html.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Either text or html body is required".to_string(),
            }),
        ));
    }

    // Validate sender email format
    if !is_valid_email(&input.from) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Invalid sender email address".to_string(),
            }),
        ));
    }

    // Validate all recipient email formats
    for recipient in input.to.iter().chain(input.cc.iter()).chain(input.bcc.iter()) {
        if !is_valid_email(recipient) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message: format!("Invalid recipient email address: {}", recipient),
                }),
            ));
        }
    }

    // Verify sender mailbox belongs to tenant (tenant-scoped query)
    let mailbox_repo = MailboxRepository::new(state.db_pool.clone());
    let _sender_mailbox = mailbox_repo
        .find_by_address_for_tenant(tenant_id, &input.from.to_lowercase())
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to verify sender mailbox".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "forbidden".to_string(),
                    message: "Sender address not found or not authorized for this tenant"
                        .to_string(),
                }),
            )
        })?;

    // Generate message ID
    let message_id = Uuid::now_v7();
    let message_id_header = input.message_id.clone().unwrap_or_else(|| {
        let domain = extract_domain(&input.from).unwrap_or("localhost");
        format!("<{}.{}@{}>", message_id, Utc::now().timestamp_millis(), domain)
    });

    // Calculate total recipients
    let recipients_count = input.to.len() + input.cc.len() + input.bcc.len();

    // Build RFC 5322 message
    let raw_message = build_message(&input, &message_id_header)?;

    // Calculate body size and validate
    let body_size = raw_message.len();
    if body_size > MAX_MESSAGE_SIZE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: format!(
                    "Message too large: {} bytes (max {} bytes)",
                    body_size, MAX_MESSAGE_SIZE
                ),
            }),
        ));
    }

    // Store message in queue
    let queue_id = enqueue_message(
        &state.db_pool,
        message_id,
        tenant_id,
        &input,
        &raw_message,
        body_size as i64,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "queue_error".to_string(),
                message: format!("Failed to queue message: {}", e),
            }),
        )
    })?;

    Ok((
        StatusCode::ACCEPTED,
        Json(SendEmailResponse {
            message_id,
            status: "queued".to_string(),
            recipients_count,
            scheduled_at: input.scheduled_at,
            queue_id: Some(queue_id),
        }),
    ))
}

/// Error response
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

/// Build RFC 5322 compliant message
///
/// NOTE: This is a simplified implementation. In production, consider using
/// a proper MIME library like `lettre` for robust email construction.
fn build_message(
    input: &SendEmailRequest,
    message_id_header: &str,
) -> Result<Vec<u8>, (StatusCode, Json<ErrorResponse>)> {
    let mut message = String::new();

    // Required headers (sanitized to prevent header injection)
    message.push_str(&format!(
        "Message-ID: {}\r\n",
        sanitize_header_value(message_id_header)
    ));
    message.push_str(&format!(
        "Date: {}\r\n",
        Utc::now().format("%a, %d %b %Y %H:%M:%S %z")
    ));
    message.push_str(&format!("From: {}\r\n", sanitize_header_value(&input.from)));
    message.push_str(&format!(
        "To: {}\r\n",
        sanitize_header_value(&input.to.join(", "))
    ));

    if !input.cc.is_empty() {
        message.push_str(&format!(
            "Cc: {}\r\n",
            sanitize_header_value(&input.cc.join(", "))
        ));
    }

    if let Some(ref subject) = input.subject {
        // Encode subject if it contains non-ASCII characters
        message.push_str(&format!("Subject: {}\r\n", encode_header_if_needed(subject)));
    }

    if let Some(ref reply_to) = input.reply_to {
        message.push_str(&format!(
            "Reply-To: {}\r\n",
            sanitize_header_value(reply_to)
        ));
    }

    // Custom headers (with strict validation)
    for (name, value) in &input.headers {
        // Only allow safe header names (alphanumeric and hyphen)
        if !is_safe_header_name(name) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message: format!("Invalid header name: {}", name),
                }),
            ));
        }
        // Don't allow overriding critical headers
        let lower_name = name.to_lowercase();
        if matches!(
            lower_name.as_str(),
            "from" | "to" | "cc" | "bcc" | "subject" | "date" | "message-id"
        ) {
            continue; // Skip, we set these ourselves
        }
        message.push_str(&format!(
            "{}: {}\r\n",
            name,
            encode_header_if_needed(value)
        ));
    }

    // MIME headers for multipart messages
    let has_attachments = !input.attachments.is_empty();
    let has_both_parts = input.text.is_some() && input.html.is_some();

    if has_attachments || has_both_parts {
        let boundary = format!("----=_Part_{}", Uuid::new_v4().simple());
        message.push_str("MIME-Version: 1.0\r\n");

        if has_attachments {
            message.push_str(&format!(
                "Content-Type: multipart/mixed; boundary=\"{}\"\r\n",
                boundary
            ));
        } else {
            message.push_str(&format!(
                "Content-Type: multipart/alternative; boundary=\"{}\"\r\n",
                boundary
            ));
        }

        message.push_str("\r\n");

        // Text part
        if let Some(ref text) = input.text {
            message.push_str(&format!("--{}\r\n", boundary));
            message.push_str("Content-Type: text/plain; charset=utf-8\r\n");
            message.push_str("Content-Transfer-Encoding: quoted-printable\r\n\r\n");
            message.push_str(text);
            message.push_str("\r\n");
        }

        // HTML part
        if let Some(ref html) = input.html {
            message.push_str(&format!("--{}\r\n", boundary));
            message.push_str("Content-Type: text/html; charset=utf-8\r\n");
            message.push_str("Content-Transfer-Encoding: quoted-printable\r\n\r\n");
            message.push_str(html);
            message.push_str("\r\n");
        }

        // Attachments
        for attachment in &input.attachments {
            message.push_str(&format!("--{}\r\n", boundary));
            message.push_str(&format!(
                "Content-Type: {}; name=\"{}\"\r\n",
                attachment.content_type, attachment.filename
            ));
            message.push_str("Content-Transfer-Encoding: base64\r\n");
            message.push_str(&format!(
                "Content-Disposition: attachment; filename=\"{}\"\r\n\r\n",
                attachment.filename
            ));
            message.push_str(&attachment.content);
            message.push_str("\r\n");
        }

        message.push_str(&format!("--{}--\r\n", boundary));
    } else {
        // Simple message
        message.push_str("MIME-Version: 1.0\r\n");

        if let Some(ref html) = input.html {
            message.push_str("Content-Type: text/html; charset=utf-8\r\n\r\n");
            message.push_str(html);
        } else if let Some(ref text) = input.text {
            message.push_str("Content-Type: text/plain; charset=utf-8\r\n\r\n");
            message.push_str(text);
        }
    }

    Ok(message.into_bytes())
}

/// Enqueue message for delivery
async fn enqueue_message(
    db_pool: &DatabasePool,
    message_id: Uuid,
    tenant_id: Uuid,
    input: &SendEmailRequest,
    raw_message: &[u8],
    body_size: i64,
) -> Result<Uuid, String> {
    let job_id = Uuid::now_v7();
    let now = Utc::now();
    let scheduled_at = input.scheduled_at.unwrap_or(now);

    // Combine all recipients
    let mut all_recipients = input.to.clone();
    all_recipients.extend(input.cc.clone());
    all_recipients.extend(input.bcc.clone());

    // Create delivery job payload
    let payload = serde_json::json!({
        "message_id": message_id,
        "tenant_id": tenant_id,
        "from": input.from,
        "to": all_recipients,
        "subject": input.subject,
        "body_size": body_size,
        "raw_message_base64": base64::engine::general_purpose::STANDARD.encode(raw_message),
    });

    // Insert job into queue
    sqlx::query(
        r#"
        INSERT INTO jobs (id, queue, payload, status, attempts, max_attempts, scheduled_at, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(job_id)
    .bind("delivery")
    .bind(&payload)
    .bind("pending")
    .bind(0i32)
    .bind(5i32)
    .bind(scheduled_at)
    .bind(now)
    .execute(db_pool.pool())
    .await
    .map_err(|e| e.to_string())?;

    Ok(job_id)
}

/// Get send queue status for a tenant
///
/// GET /api/v1/tenants/:tenant_id/send/queue
pub async fn get_send_queue(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<QueueStatusResponse>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let stats = get_queue_stats(&state.db_pool, tenant_id)
        .await
        .map_err(|e| {
            error!("Database error while getting queue stats: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(stats))
}

/// Queue status response
#[derive(Debug, Clone, Serialize)]
pub struct QueueStatusResponse {
    pub pending: i64,
    pub processing: i64,
    pub completed: i64,
    pub failed: i64,
}

/// Get queue statistics
async fn get_queue_stats(db_pool: &DatabasePool, tenant_id: Uuid) -> Result<QueueStatusResponse, String> {
    let pool = db_pool.pool();
    let tenant_id_str = tenant_id.to_string();

    // Filter by tenant_id from the job payload using JSONB query
    let pending: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM jobs
           WHERE status = 'pending' AND queue = 'delivery'
           AND payload->>'tenant_id' = $1"#,
    )
    .bind(&tenant_id_str)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let processing: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM jobs
           WHERE status = 'processing' AND queue = 'delivery'
           AND payload->>'tenant_id' = $1"#,
    )
    .bind(&tenant_id_str)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let completed: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM jobs
           WHERE status = 'completed' AND queue = 'delivery'
           AND payload->>'tenant_id' = $1"#,
    )
    .bind(&tenant_id_str)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let failed: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM jobs
           WHERE status = 'failed' AND queue = 'delivery'
           AND payload->>'tenant_id' = $1"#,
    )
    .bind(&tenant_id_str)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(QueueStatusResponse {
        pending: pending.0,
        processing: processing.0,
        completed: completed.0,
        failed: failed.0,
    })
}

/// Get status of a specific queued message
///
/// GET /api/v1/tenants/:tenant_id/send/:message_id/status
pub async fn get_message_status(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<MessageStatusResponse>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let pool = state.db_pool.pool();

    // Find job by message_id in payload AND verify tenant ownership
    let job: Option<(String, i32, Option<String>, chrono::DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT status, attempts, last_error, scheduled_at
        FROM jobs
        WHERE queue = 'delivery'
          AND payload->>'message_id' = $1
          AND payload->>'tenant_id' = $2
        "#,
    )
    .bind(message_id.to_string())
    .bind(tenant_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!("Database error while fetching message status: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match job {
        Some((status, attempts, last_error, scheduled_at)) => Ok(Json(MessageStatusResponse {
            message_id,
            status,
            attempts,
            last_error,
            scheduled_at: Some(scheduled_at),
        })),
        None => {
            warn!(
                "Message {} not found or not owned by tenant {}",
                message_id, tenant_id
            );
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// Message status response
#[derive(Debug, Clone, Serialize)]
pub struct MessageStatusResponse {
    pub message_id: Uuid,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub scheduled_at: Option<chrono::DateTime<Utc>>,
}

/// Validate an attachment
///
/// Checks:
/// - Filename is valid (no path traversal, reasonable length)
/// - Content-Type is reasonable
/// - Base64 content is valid and within size limits
fn validate_attachment(attachment: &Attachment) -> Result<(), String> {
    // Validate filename
    if attachment.filename.is_empty() {
        return Err("Filename is required".to_string());
    }
    if attachment.filename.len() > 255 {
        return Err("Filename too long (max 255 characters)".to_string());
    }
    // Check for path traversal attempts
    if attachment.filename.contains("..") || attachment.filename.contains('/') || attachment.filename.contains('\\') {
        return Err("Invalid filename: path traversal not allowed".to_string());
    }
    // Check for null bytes
    if attachment.filename.contains('\0') {
        return Err("Invalid filename: null bytes not allowed".to_string());
    }

    // Validate content type
    if attachment.content_type.is_empty() {
        return Err("Content-Type is required".to_string());
    }
    if attachment.content_type.len() > 255 {
        return Err("Content-Type too long".to_string());
    }
    // Basic content-type format validation
    if !attachment.content_type.contains('/') {
        return Err("Invalid Content-Type format".to_string());
    }
    // Block potentially dangerous content types
    let dangerous_types = ["application/x-msdownload", "application/x-executable"];
    let lower_type = attachment.content_type.to_lowercase();
    for dangerous in dangerous_types {
        if lower_type.starts_with(dangerous) {
            return Err(format!("Blocked content type: {}", attachment.content_type));
        }
    }

    // Validate base64 content
    if attachment.content.is_empty() {
        return Err("Content is required".to_string());
    }

    // Try to decode the base64 to validate it
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&attachment.content)
        .map_err(|e| format!("Invalid base64 content: {}", e))?;

    // Check decoded size
    if decoded.len() > MAX_ATTACHMENT_SIZE {
        return Err(format!(
            "Attachment too large: {} bytes (max {} bytes)",
            decoded.len(),
            MAX_ATTACHMENT_SIZE
        ));
    }

    Ok(())
}

/// Sanitize a header value to prevent header injection
///
/// Removes or escapes characters that could be used for header injection
fn sanitize_header_value(value: &str) -> String {
    // Remove carriage returns and newlines that could be used for header injection
    value
        .chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .collect()
}

/// Encode a header value using RFC 2047 if it contains non-ASCII characters
///
/// This is a simplified implementation. In production, use a proper MIME library.
fn encode_header_if_needed(value: &str) -> String {
    // Check if the value contains non-ASCII characters
    if value.is_ascii() {
        return sanitize_header_value(value);
    }

    // Use RFC 2047 Base64 encoding for non-ASCII values
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(sanitize_header_value(value).as_bytes());
    format!("=?UTF-8?B?{}?=", encoded)
}

/// Check if a header name is safe (alphanumeric and hyphen only)
fn is_safe_header_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 100 {
        return false;
    }

    // Must start with a letter
    if !name.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
        return false;
    }

    // All characters must be alphanumeric or hyphen
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
