//! Message indexer for search

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::client::{MeilisearchClient, SearchRequest, SearchResult};

/// Document structure for indexing messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDocument {
    /// Message ID (primary key)
    pub id: String,
    /// Tenant ID for filtering
    pub tenant_id: String,
    /// Mailbox ID for filtering
    pub mailbox_id: String,
    /// Message-ID header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id_header: Option<String>,
    /// Email subject
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Sender address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_address: Option<String>,
    /// Recipient addresses
    pub to_addresses: Vec<String>,
    /// CC addresses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_addresses: Option<Vec<String>>,
    /// Body preview (first 4KB of text)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_preview: Option<String>,
    /// Has attachments flag
    pub has_attachments: bool,
    /// Read/seen flag
    pub seen: bool,
    /// Flagged/starred
    pub flagged: bool,
    /// Tags
    pub tags: Vec<String>,
    /// Spam score
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spam_score: Option<f64>,
    /// Received timestamp (Unix timestamp for sorting)
    pub received_at: i64,
    /// Received timestamp as ISO string
    pub received_at_iso: String,
}

impl MessageDocument {
    /// Create a new message document from message data
    pub fn new(
        id: Uuid,
        tenant_id: Uuid,
        mailbox_id: Uuid,
        message_id_header: Option<String>,
        subject: Option<String>,
        from_address: Option<String>,
        to_addresses: Vec<String>,
        cc_addresses: Option<Vec<String>>,
        body_preview: Option<String>,
        has_attachments: bool,
        seen: bool,
        flagged: bool,
        tags: Vec<String>,
        spam_score: Option<f64>,
        received_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            mailbox_id: mailbox_id.to_string(),
            message_id_header,
            subject,
            from_address,
            to_addresses,
            cc_addresses,
            body_preview,
            has_attachments,
            seen,
            flagged,
            tags,
            spam_score,
            received_at: received_at.timestamp(),
            received_at_iso: received_at.to_rfc3339(),
        }
    }
}

/// Search hit result with highlights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSearchHit {
    #[serde(flatten)]
    pub document: MessageDocument,
    /// Highlighted fields (if requested)
    #[serde(rename = "_formatted")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatted: Option<FormattedFields>,
}

/// Highlighted/formatted fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_preview: Option<String>,
}

/// Message search options
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Search query
    pub query: String,
    /// Tenant ID filter (required)
    pub tenant_id: Uuid,
    /// Optional mailbox ID filter
    pub mailbox_id: Option<Uuid>,
    /// Filter by sender address
    pub from_address: Option<String>,
    /// Filter by has_attachments
    pub has_attachments: Option<bool>,
    /// Filter by seen status
    pub seen: Option<bool>,
    /// Filter by flagged status
    pub flagged: Option<bool>,
    /// Filter by tags (any match)
    pub tags: Option<Vec<String>>,
    /// Date range start
    pub date_from: Option<DateTime<Utc>>,
    /// Date range end
    pub date_to: Option<DateTime<Utc>>,
    /// Result offset
    pub offset: Option<u64>,
    /// Result limit
    pub limit: Option<u64>,
}

/// Message indexer for managing search index
pub struct MessageIndexer {
    client: MeilisearchClient,
}

impl MessageIndexer {
    /// Create a new message indexer
    pub fn new(client: MeilisearchClient) -> Self {
        Self { client }
    }

    /// Initialize the search index (create and configure)
    pub async fn initialize(&self) -> Result<(), String> {
        self.client.setup_index().await
    }

    /// Check if search service is available
    pub async fn is_available(&self) -> bool {
        match self.client.health_check().await {
            Ok(true) => true,
            Ok(false) => {
                warn!("Search service reported unhealthy status");
                false
            }
            Err(e) => {
                error!("Search health check failed: {}", e);
                false
            }
        }
    }

    /// Index a single message
    pub async fn index_message(&self, document: MessageDocument) -> Result<u64, String> {
        let documents = vec![document];
        let task = self.client.index_documents(&documents).await?;
        Ok(task.task_uid)
    }

    /// Index multiple messages
    pub async fn index_messages(&self, documents: Vec<MessageDocument>) -> Result<u64, String> {
        if documents.is_empty() {
            return Ok(0);
        }

        info!("Indexing {} messages", documents.len());
        let task = self.client.index_documents(&documents).await?;
        Ok(task.task_uid)
    }

    /// Delete a message from the index
    pub async fn delete_message(&self, message_id: Uuid) -> Result<u64, String> {
        let task = self.client.delete_document(&message_id.to_string()).await?;
        Ok(task.task_uid)
    }

    /// Delete all messages for a tenant
    pub async fn delete_tenant_messages(&self, tenant_id: Uuid) -> Result<u64, String> {
        let filter = format!("tenant_id = '{}'", tenant_id);
        let task = self.client.delete_by_filter(&filter).await?;
        info!("Deleting messages for tenant {}", tenant_id);
        Ok(task.task_uid)
    }

    /// Delete all messages for a mailbox
    pub async fn delete_mailbox_messages(&self, mailbox_id: Uuid) -> Result<u64, String> {
        let filter = format!("mailbox_id = '{}'", mailbox_id);
        let task = self.client.delete_by_filter(&filter).await?;
        info!("Deleting messages for mailbox {}", mailbox_id);
        Ok(task.task_uid)
    }

    /// Search for messages
    pub async fn search(
        &self,
        options: SearchOptions,
    ) -> Result<SearchResult<MessageSearchHit>, String> {
        // Build filter string
        let mut filters = vec![format!("tenant_id = '{}'", options.tenant_id)];

        if let Some(mailbox_id) = options.mailbox_id {
            filters.push(format!("mailbox_id = '{}'", mailbox_id));
        }

        if let Some(from) = options.from_address {
            filters.push(format!("from_address = '{}'", from));
        }

        if let Some(has_attachments) = options.has_attachments {
            filters.push(format!("has_attachments = {}", has_attachments));
        }

        if let Some(seen) = options.seen {
            filters.push(format!("seen = {}", seen));
        }

        if let Some(flagged) = options.flagged {
            filters.push(format!("flagged = {}", flagged));
        }

        if let Some(tags) = options.tags {
            if !tags.is_empty() {
                let tag_conditions: Vec<String> =
                    tags.iter().map(|t| format!("tags = '{}'", t)).collect();
                filters.push(format!("({})", tag_conditions.join(" OR ")));
            }
        }

        if let Some(date_from) = options.date_from {
            filters.push(format!("received_at >= {}", date_from.timestamp()));
        }

        if let Some(date_to) = options.date_to {
            filters.push(format!("received_at <= {}", date_to.timestamp()));
        }

        let filter = filters.join(" AND ");
        debug!("Search filter: {}", filter);

        let request = SearchRequest {
            q: options.query,
            offset: options.offset,
            limit: options.limit.or(Some(20)),
            filter: Some(filter),
            attributes_to_retrieve: None,
            attributes_to_highlight: Some(vec!["subject".to_string(), "body_preview".to_string()]),
            sort: Some(vec!["received_at:desc".to_string()]),
        };

        self.client.search(request).await
    }

    /// Update message flags in the index
    pub async fn update_message_flags(
        &self,
        message_id: Uuid,
        tenant_id: Uuid,
        mailbox_id: Uuid,
        seen: bool,
        flagged: bool,
    ) -> Result<u64, String> {
        // Meilisearch updates documents by re-indexing with same ID
        // We only need to send the fields we want to update + primary key
        let partial_doc = serde_json::json!({
            "id": message_id.to_string(),
            "tenant_id": tenant_id.to_string(),
            "mailbox_id": mailbox_id.to_string(),
            "seen": seen,
            "flagged": flagged
        });

        let documents = vec![partial_doc];
        let task = self.client.index_documents(&documents).await?;
        debug!("Updated flags for message {}", message_id);
        Ok(task.task_uid)
    }

    /// Get task status
    pub async fn get_task_status(&self, task_uid: u64) -> Result<serde_json::Value, String> {
        self.client.get_task(task_uid).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_document_creation() {
        let now = Utc::now();
        let doc = MessageDocument::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some("msg-id".to_string()),
            Some("Test Subject".to_string()),
            Some("sender@example.com".to_string()),
            vec!["recipient@example.com".to_string()],
            None,
            Some("Body preview text".to_string()),
            false,
            false,
            false,
            vec!["inbox".to_string()],
            Some(0.5),
            now,
        );

        assert_eq!(doc.subject, Some("Test Subject".to_string()));
        assert_eq!(doc.received_at, now.timestamp());
    }

    #[test]
    fn test_search_options_default() {
        let options = SearchOptions::default();
        assert!(options.query.is_empty());
        assert!(options.mailbox_id.is_none());
        assert!(options.has_attachments.is_none());
    }
}
