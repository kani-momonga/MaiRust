//! Message repository

use crate::db::DatabasePool;
use crate::models::{CreateMessage, Message};
use async_trait::async_trait;
use mairust_common::types::{MailboxId, MessageId, TenantId};
use mairust_common::{Error, Result};
use sqlx::Row;
use uuid::Uuid;

/// Message repository trait
#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// Create a new message
    async fn create(&self, input: CreateMessage) -> Result<Message>;

    /// Get a message by ID
    async fn get(&self, tenant_id: TenantId, id: MessageId) -> Result<Option<Message>>;

    /// List messages in a mailbox
    async fn list(
        &self,
        tenant_id: TenantId,
        mailbox_id: MailboxId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Message>>;

    /// Update message flags
    async fn update_flags(
        &self,
        tenant_id: TenantId,
        id: MessageId,
        seen: Option<bool>,
        flagged: Option<bool>,
        answered: Option<bool>,
        deleted: Option<bool>,
    ) -> Result<()>;

    /// Add tags to a message
    async fn add_tags(&self, tenant_id: TenantId, id: MessageId, tags: Vec<String>) -> Result<()>;

    /// Remove tags from a message
    async fn remove_tags(
        &self,
        tenant_id: TenantId,
        id: MessageId,
        tags: Vec<String>,
    ) -> Result<()>;

    /// Delete a message (soft delete)
    async fn delete(&self, tenant_id: TenantId, id: MessageId) -> Result<()>;

    /// Count messages in a mailbox
    async fn count(&self, tenant_id: TenantId, mailbox_id: MailboxId) -> Result<i64>;

    /// Count unread messages in a mailbox
    async fn count_unread(&self, tenant_id: TenantId, mailbox_id: MailboxId) -> Result<i64>;
}

/// PostgreSQL/SQLite message repository implementation
pub struct DbMessageRepository {
    pool: DatabasePool,
}

impl DbMessageRepository {
    /// Create a new repository
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Find a message by ID (alias for get)
    pub async fn find_by_id(&self, id: MessageId) -> Result<Option<Message>> {
        sqlx::query_as::<_, Message>(
            "SELECT * FROM messages WHERE id = $1 AND deleted = false",
        )
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    /// Find messages by mailbox (simplified version for handlers)
    pub async fn find_by_mailbox(&self, mailbox_id: MailboxId, limit: usize) -> Result<Vec<Message>> {
        sqlx::query_as::<_, Message>(
            r#"
            SELECT * FROM messages
            WHERE mailbox_id = $1 AND deleted = false
            ORDER BY received_at DESC
            LIMIT $2
            "#,
        )
        .bind(mailbox_id)
        .bind(limit as i64)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    /// Create a message directly from Message struct
    pub async fn create(&self, message: &Message) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO messages (
                id, tenant_id, mailbox_id, message_id_header, subject,
                from_address, to_addresses, cc_addresses, headers, body_preview,
                body_size, has_attachments, storage_path, seen, answered,
                flagged, deleted, draft, spam_score, tags, metadata, received_at, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23
            )
            "#,
        )
        .bind(message.id)
        .bind(message.tenant_id)
        .bind(message.mailbox_id)
        .bind(&message.message_id_header)
        .bind(&message.subject)
        .bind(&message.from_address)
        .bind(&message.to_addresses)
        .bind(&message.cc_addresses)
        .bind(&message.headers)
        .bind(&message.body_preview)
        .bind(message.body_size)
        .bind(message.has_attachments)
        .bind(&message.storage_path)
        .bind(message.seen)
        .bind(message.answered)
        .bind(message.flagged)
        .bind(message.deleted)
        .bind(message.draft)
        .bind(message.spam_score)
        .bind(&message.tags)
        .bind(&message.metadata)
        .bind(message.received_at)
        .bind(message.created_at)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    /// Update flags (simplified for handlers)
    pub async fn update_flags(
        &self,
        id: MessageId,
        seen: Option<bool>,
        answered: Option<bool>,
        flagged: Option<bool>,
        deleted: Option<bool>,
    ) -> Result<()> {
        let mut updates = vec!["updated_at = NOW()".to_string()];
        let mut idx = 2;

        if seen.is_some() {
            updates.push(format!("seen = ${}", idx));
            idx += 1;
        }
        if answered.is_some() {
            updates.push(format!("answered = ${}", idx));
            idx += 1;
        }
        if flagged.is_some() {
            updates.push(format!("flagged = ${}", idx));
            idx += 1;
        }
        if deleted.is_some() {
            updates.push(format!("deleted = ${}", idx));
        }

        let query = format!("UPDATE messages SET {} WHERE id = $1", updates.join(", "));
        let mut q = sqlx::query(&query).bind(id);

        if let Some(v) = seen {
            q = q.bind(v);
        }
        if let Some(v) = answered {
            q = q.bind(v);
        }
        if let Some(v) = flagged {
            q = q.bind(v);
        }
        if let Some(v) = deleted {
            q = q.bind(v);
        }

        q.execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    /// Delete a message (soft delete)
    pub async fn delete(&self, id: MessageId) -> Result<()> {
        self.update_flags(id, None, None, None, Some(true)).await
    }
}

#[async_trait]
impl MessageRepository for DbMessageRepository {
    async fn create(&self, input: CreateMessage) -> Result<Message> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();

        let to_json = serde_json::to_value(&input.to_addresses)
            .map_err(|e| Error::Internal(e.to_string()))?;
        let cc_json = input
            .cc_addresses
            .as_ref()
            .map(|cc| serde_json::to_value(cc))
            .transpose()
            .map_err(|e| Error::Internal(e.to_string()))?;
        let tags_json = serde_json::Value::Array(vec![]);
        let metadata_json = serde_json::Value::Object(serde_json::Map::new());

        sqlx::query(
            r#"
            INSERT INTO messages (
                id, tenant_id, mailbox_id, message_id_header, subject,
                from_address, to_addresses, cc_addresses, headers, body_preview,
                body_size, has_attachments, storage_path, seen, answered,
                flagged, deleted, draft, tags, metadata, received_at, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22
            )
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(input.mailbox_id)
        .bind(&input.message_id_header)
        .bind(&input.subject)
        .bind(&input.from_address)
        .bind(&to_json)
        .bind(&cc_json)
        .bind(&input.headers)
        .bind(&input.body_preview)
        .bind(input.body_size)
        .bind(input.has_attachments)
        .bind(&input.storage_path)
        .bind(false) // seen
        .bind(false) // answered
        .bind(false) // flagged
        .bind(false) // deleted
        .bind(false) // draft
        .bind(&tags_json)
        .bind(&metadata_json)
        .bind(input.received_at)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(input.tenant_id, id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to retrieve created message".to_string()))
    }

    async fn get(&self, tenant_id: TenantId, id: MessageId) -> Result<Option<Message>> {
        let message = sqlx::query_as::<_, Message>(
            r#"
            SELECT * FROM messages
            WHERE tenant_id = $1 AND id = $2 AND deleted = false
            "#,
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(message)
    }

    async fn list(
        &self,
        tenant_id: TenantId,
        mailbox_id: MailboxId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Message>> {
        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT * FROM messages
            WHERE tenant_id = $1 AND mailbox_id = $2 AND deleted = false
            ORDER BY received_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(tenant_id)
        .bind(mailbox_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(messages)
    }

    async fn update_flags(
        &self,
        tenant_id: TenantId,
        id: MessageId,
        seen: Option<bool>,
        flagged: Option<bool>,
        answered: Option<bool>,
        deleted: Option<bool>,
    ) -> Result<()> {
        let mut query = String::from("UPDATE messages SET updated_at = NOW()");
        let mut param_idx = 3;

        if seen.is_some() {
            query.push_str(&format!(", seen = ${}", param_idx));
            param_idx += 1;
        }
        if flagged.is_some() {
            query.push_str(&format!(", flagged = ${}", param_idx));
            param_idx += 1;
        }
        if answered.is_some() {
            query.push_str(&format!(", answered = ${}", param_idx));
            param_idx += 1;
        }
        if deleted.is_some() {
            query.push_str(&format!(", deleted = ${}", param_idx));
        }

        query.push_str(" WHERE tenant_id = $1 AND id = $2");

        let mut q = sqlx::query(&query).bind(tenant_id).bind(id);

        if let Some(v) = seen {
            q = q.bind(v);
        }
        if let Some(v) = flagged {
            q = q.bind(v);
        }
        if let Some(v) = answered {
            q = q.bind(v);
        }
        if let Some(v) = deleted {
            q = q.bind(v);
        }

        q.execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    async fn add_tags(&self, tenant_id: TenantId, id: MessageId, tags: Vec<String>) -> Result<()> {
        // Get current tags and merge
        let message = self
            .get(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound("Message not found".to_string()))?;

        let mut current_tags = message.tags_vec();
        for tag in tags {
            if !current_tags.contains(&tag) {
                current_tags.push(tag);
            }
        }

        let tags_json =
            serde_json::to_value(&current_tags).map_err(|e| Error::Internal(e.to_string()))?;

        sqlx::query("UPDATE messages SET tags = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(id)
            .bind(tags_json)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    async fn remove_tags(
        &self,
        tenant_id: TenantId,
        id: MessageId,
        tags: Vec<String>,
    ) -> Result<()> {
        let message = self
            .get(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound("Message not found".to_string()))?;

        let current_tags: Vec<String> = message
            .tags_vec()
            .into_iter()
            .filter(|t| !tags.contains(t))
            .collect();

        let tags_json =
            serde_json::to_value(&current_tags).map_err(|e| Error::Internal(e.to_string()))?;

        sqlx::query("UPDATE messages SET tags = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(id)
            .bind(tags_json)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, _tenant_id: TenantId, id: MessageId) -> Result<()> {
        DbMessageRepository::update_flags(self, id, None, None, None, Some(true)).await
    }

    async fn count(&self, tenant_id: TenantId, mailbox_id: MailboxId) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM messages
            WHERE tenant_id = $1 AND mailbox_id = $2 AND deleted = false
            "#,
        )
        .bind(tenant_id)
        .bind(mailbox_id)
        .fetch_one(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(row.get::<i64, _>("count"))
    }

    async fn count_unread(&self, tenant_id: TenantId, mailbox_id: MailboxId) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM messages
            WHERE tenant_id = $1 AND mailbox_id = $2 AND deleted = false AND seen = false
            "#,
        )
        .bind(tenant_id)
        .bind(mailbox_id)
        .fetch_one(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(row.get::<i64, _>("count"))
    }
}
