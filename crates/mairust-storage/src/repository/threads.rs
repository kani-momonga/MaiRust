//! Thread repository
//!
//! Database operations for message threading.

use crate::db::DatabasePool;
use crate::models::{CreateThread, Thread};
use anyhow::Result;
use mairust_common::types::{MailboxId, MessageId, TenantId};
use uuid::Uuid;

/// Thread repository
pub struct ThreadRepository {
    pool: DatabasePool,
}

impl ThreadRepository {
    /// Create a new thread repository
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Create a new thread
    pub async fn create(&self, input: CreateThread) -> Result<Thread> {
        let thread_id = Uuid::new_v4();

        let thread = sqlx::query_as::<_, Thread>(
            r#"
            INSERT INTO threads (id, tenant_id, mailbox_id, subject, participant_addresses,
                message_count, unread_count, created_at, updated_at)
            VALUES ($1, $2, $3, $4, '[]', 0, 0, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(thread_id)
        .bind(input.tenant_id)
        .bind(input.mailbox_id)
        .bind(&input.subject)
        .fetch_one(self.pool.pool())
        .await?;

        Ok(thread)
    }

    /// Get a thread by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<Thread>> {
        let thread = sqlx::query_as::<_, Thread>("SELECT * FROM threads WHERE id = $1")
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await?;

        Ok(thread)
    }

    /// Get threads for a mailbox
    pub async fn list_by_mailbox(
        &self,
        tenant_id: TenantId,
        mailbox_id: MailboxId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Thread>> {
        let threads = sqlx::query_as::<_, Thread>(
            r#"
            SELECT * FROM threads
            WHERE tenant_id = $1 AND mailbox_id = $2
            ORDER BY last_message_at DESC NULLS LAST
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(tenant_id)
        .bind(mailbox_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(threads)
    }

    /// Find or create thread by message headers
    pub async fn find_or_create_thread(
        &self,
        tenant_id: TenantId,
        mailbox_id: MailboxId,
        message_id_header: Option<&str>,
        in_reply_to: Option<&str>,
        references: Option<&str>,
        subject: Option<&str>,
    ) -> Result<Uuid> {
        let pool = self.pool.pool();

        // Try to find existing thread by References or In-Reply-To
        if let Some(refs) = references {
            // Parse references header (space-separated message IDs)
            let ref_ids: Vec<&str> = refs.split_whitespace().collect();

            for ref_id in ref_ids.iter().rev() {
                let existing: Option<(Uuid,)> = sqlx::query_as(
                    r#"
                    SELECT thread_id FROM messages
                    WHERE tenant_id = $1 AND message_id_header = $2 AND thread_id IS NOT NULL
                    LIMIT 1
                    "#,
                )
                .bind(tenant_id)
                .bind(ref_id)
                .fetch_optional(pool)
                .await?;

                if let Some((thread_id,)) = existing {
                    return Ok(thread_id);
                }
            }
        }

        // Try by In-Reply-To
        if let Some(reply_to) = in_reply_to {
            let existing: Option<(Uuid,)> = sqlx::query_as(
                r#"
                SELECT thread_id FROM messages
                WHERE tenant_id = $1 AND message_id_header = $2 AND thread_id IS NOT NULL
                LIMIT 1
                "#,
            )
            .bind(tenant_id)
            .bind(reply_to)
            .fetch_optional(pool)
            .await?;

            if let Some((thread_id,)) = existing {
                return Ok(thread_id);
            }
        }

        // Create new thread
        let thread_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO threads (id, tenant_id, mailbox_id, subject, participant_addresses,
                message_count, unread_count, created_at, updated_at)
            VALUES ($1, $2, $3, $4, '[]', 0, 0, NOW(), NOW())
            "#,
        )
        .bind(thread_id)
        .bind(tenant_id)
        .bind(mailbox_id)
        .bind(subject)
        .execute(pool)
        .await?;

        Ok(thread_id)
    }

    /// Update thread statistics
    pub async fn update_thread_stats(&self, thread_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE threads SET
                message_count = (SELECT COUNT(*) FROM messages WHERE thread_id = $1),
                unread_count = (SELECT COUNT(*) FROM messages WHERE thread_id = $1 AND seen = false),
                first_message_at = (SELECT MIN(received_at) FROM messages WHERE thread_id = $1),
                last_message_at = (SELECT MAX(received_at) FROM messages WHERE thread_id = $1),
                last_message_id = (
                    SELECT id FROM messages
                    WHERE thread_id = $1
                    ORDER BY received_at DESC
                    LIMIT 1
                ),
                participant_addresses = (
                    SELECT COALESCE(jsonb_agg(DISTINCT from_address), '[]')
                    FROM messages
                    WHERE thread_id = $1 AND from_address IS NOT NULL
                ),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(thread_id)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Get messages in a thread
    pub async fn get_thread_messages(&self, thread_id: Uuid) -> Result<Vec<MessageId>> {
        let messages: Vec<(MessageId,)> = sqlx::query_as(
            r#"
            SELECT id FROM messages
            WHERE thread_id = $1
            ORDER BY received_at ASC
            "#,
        )
        .bind(thread_id)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(messages.into_iter().map(|(id,)| id).collect())
    }

    /// Delete a thread
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM threads WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
