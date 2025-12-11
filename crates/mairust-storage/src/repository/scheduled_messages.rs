//! Scheduled message repository

use chrono::{DateTime, Utc};
use mairust_common::types::TenantId;
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

use crate::models::{CreateScheduledMessage, ScheduledMessage, ScheduledMessageStatus};

/// Scheduled message repository
#[derive(Clone)]
pub struct ScheduledMessageRepository {
    pool: PgPool,
}

impl ScheduledMessageRepository {
    /// Create a new scheduled message repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get the database pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Create a new scheduled message
    pub async fn create(
        &self,
        input: CreateScheduledMessage,
    ) -> Result<ScheduledMessage, sqlx::Error> {
        let id = Uuid::new_v4();
        let headers = input.headers.unwrap_or_else(|| serde_json::json!({}));
        let metadata = input.metadata.unwrap_or_else(|| serde_json::json!({}));

        sqlx::query_as::<_, ScheduledMessage>(
            r#"
            INSERT INTO scheduled_messages (
                id, tenant_id, campaign_id, recipient_id, batch_id,
                from_address, to_address, subject, html_body, text_body,
                headers, scheduled_at, max_attempts, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(input.campaign_id)
        .bind(input.recipient_id)
        .bind(input.batch_id)
        .bind(&input.from_address)
        .bind(&input.to_address)
        .bind(&input.subject)
        .bind(&input.html_body)
        .bind(&input.text_body)
        .bind(&headers)
        .bind(input.scheduled_at)
        .bind(input.max_attempts.unwrap_or(3))
        .bind(&metadata)
        .fetch_one(&self.pool)
        .await
    }

    /// Create multiple scheduled messages in batch
    pub async fn create_batch(
        &self,
        messages: Vec<CreateScheduledMessage>,
    ) -> Result<u64, sqlx::Error> {
        let mut count = 0u64;
        let mut tx = self.pool.begin().await?;

        for input in messages {
            let id = Uuid::new_v4();
            let headers = input.headers.unwrap_or_else(|| serde_json::json!({}));
            let metadata = input.metadata.unwrap_or_else(|| serde_json::json!({}));

            let result = sqlx::query(
                r#"
                INSERT INTO scheduled_messages (
                    id, tenant_id, campaign_id, recipient_id, batch_id,
                    from_address, to_address, subject, html_body, text_body,
                    headers, scheduled_at, max_attempts, metadata
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                "#,
            )
            .bind(id)
            .bind(input.tenant_id)
            .bind(input.campaign_id)
            .bind(input.recipient_id)
            .bind(input.batch_id)
            .bind(&input.from_address)
            .bind(&input.to_address)
            .bind(&input.subject)
            .bind(&input.html_body)
            .bind(&input.text_body)
            .bind(&headers)
            .bind(input.scheduled_at)
            .bind(input.max_attempts.unwrap_or(3))
            .bind(&metadata)
            .execute(&mut *tx)
            .await?;

            count += result.rows_affected();
        }

        tx.commit().await?;
        Ok(count)
    }

    /// Get a scheduled message by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>("SELECT * FROM scheduled_messages WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Get a scheduled message by ID and tenant
    pub async fn get_by_tenant(
        &self,
        tenant_id: TenantId,
        id: Uuid,
    ) -> Result<Option<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>(
            "SELECT * FROM scheduled_messages WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// List scheduled messages for a tenant
    pub async fn list_by_tenant(
        &self,
        tenant_id: TenantId,
        status: Option<ScheduledMessageStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ScheduledMessage>, sqlx::Error> {
        if let Some(status) = status {
            sqlx::query_as::<_, ScheduledMessage>(
                r#"
                SELECT * FROM scheduled_messages
                WHERE tenant_id = $1 AND status = $2
                ORDER BY scheduled_at ASC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(tenant_id)
            .bind(status.to_string())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, ScheduledMessage>(
                r#"
                SELECT * FROM scheduled_messages
                WHERE tenant_id = $1
                ORDER BY scheduled_at ASC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
    }

    /// List scheduled messages for a campaign
    pub async fn list_by_campaign(
        &self,
        campaign_id: Uuid,
        status: Option<ScheduledMessageStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ScheduledMessage>, sqlx::Error> {
        if let Some(status) = status {
            sqlx::query_as::<_, ScheduledMessage>(
                r#"
                SELECT * FROM scheduled_messages
                WHERE campaign_id = $1 AND status = $2
                ORDER BY scheduled_at ASC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(campaign_id)
            .bind(status.to_string())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, ScheduledMessage>(
                r#"
                SELECT * FROM scheduled_messages
                WHERE campaign_id = $1
                ORDER BY scheduled_at ASC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(campaign_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
    }

    /// Get pending messages ready to send (scheduled time has passed)
    /// Uses FOR UPDATE SKIP LOCKED for concurrent worker safety
    pub async fn get_pending_ready(&self, limit: i64) -> Result<Vec<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>(
            r#"
            SELECT * FROM scheduled_messages
            WHERE status = 'pending'
              AND scheduled_at <= NOW()
            ORDER BY scheduled_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    /// Mark a message as processing
    pub async fn mark_processing(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE scheduled_messages SET
                status = 'processing',
                last_attempt_at = NOW(),
                attempts = attempts + 1,
                updated_at = NOW()
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Mark a message as sent
    pub async fn mark_sent(
        &self,
        id: Uuid,
        message_id: &str,
    ) -> Result<Option<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>(
            r#"
            UPDATE scheduled_messages SET
                status = 'sent',
                message_id = $2,
                sent_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Mark a message as delivered
    pub async fn mark_delivered(&self, id: Uuid) -> Result<Option<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>(
            r#"
            UPDATE scheduled_messages SET
                status = 'delivered',
                delivered_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Mark a message as bounced
    pub async fn mark_bounced(
        &self,
        id: Uuid,
        bounce_type: &str,
        bounce_reason: &str,
    ) -> Result<Option<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>(
            r#"
            UPDATE scheduled_messages SET
                status = 'bounced',
                bounced_at = NOW(),
                bounce_type = $2,
                bounce_reason = $3,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(bounce_type)
        .bind(bounce_reason)
        .fetch_optional(&self.pool)
        .await
    }

    /// Mark a message as failed
    pub async fn mark_failed(
        &self,
        id: Uuid,
        error: &str,
    ) -> Result<Option<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>(
            r#"
            UPDATE scheduled_messages SET
                status = CASE
                    WHEN attempts < max_attempts THEN 'pending'
                    ELSE 'failed'
                END,
                last_error = $2,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(error)
        .fetch_optional(&self.pool)
        .await
    }

    /// Reschedule a message for retry
    pub async fn reschedule(
        &self,
        id: Uuid,
        scheduled_at: DateTime<Utc>,
    ) -> Result<Option<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>(
            r#"
            UPDATE scheduled_messages SET
                status = 'pending',
                scheduled_at = $2,
                updated_at = NOW()
            WHERE id = $1 AND attempts < max_attempts
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(scheduled_at)
        .fetch_optional(&self.pool)
        .await
    }

    /// Cancel a scheduled message
    pub async fn cancel(&self, id: Uuid) -> Result<Option<ScheduledMessage>, sqlx::Error> {
        sqlx::query_as::<_, ScheduledMessage>(
            r#"
            UPDATE scheduled_messages SET
                status = 'cancelled',
                updated_at = NOW()
            WHERE id = $1 AND status = 'pending'
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Cancel all pending messages for a campaign
    pub async fn cancel_by_campaign(&self, campaign_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE scheduled_messages SET
                status = 'cancelled',
                updated_at = NOW()
            WHERE campaign_id = $1 AND status = 'pending'
            "#,
        )
        .bind(campaign_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete a scheduled message
    pub async fn delete(&self, id: Uuid, tenant_id: TenantId) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM scheduled_messages WHERE id = $1 AND tenant_id = $2 AND status IN ('pending', 'cancelled')",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Count scheduled messages by tenant
    pub async fn count_by_tenant(
        &self,
        tenant_id: TenantId,
        status: Option<ScheduledMessageStatus>,
    ) -> Result<i64, sqlx::Error> {
        let count: (i64,) = if let Some(status) = status {
            sqlx::query_as(
                "SELECT COUNT(*) FROM scheduled_messages WHERE tenant_id = $1 AND status = $2",
            )
            .bind(tenant_id)
            .bind(status.to_string())
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM scheduled_messages WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(count.0)
    }

    /// Count scheduled messages by campaign and status
    pub async fn count_by_campaign(
        &self,
        campaign_id: Uuid,
        status: Option<ScheduledMessageStatus>,
    ) -> Result<i64, sqlx::Error> {
        let count: (i64,) = if let Some(status) = status {
            sqlx::query_as(
                "SELECT COUNT(*) FROM scheduled_messages WHERE campaign_id = $1 AND status = $2",
            )
            .bind(campaign_id)
            .bind(status.to_string())
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM scheduled_messages WHERE campaign_id = $1")
                .bind(campaign_id)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(count.0)
    }

    /// Get count by status for a campaign (for stats)
    pub async fn get_campaign_status_counts(
        &self,
        campaign_id: Uuid,
    ) -> Result<CampaignMessageCounts, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE status = 'pending') as pending,
                COUNT(*) FILTER (WHERE status = 'processing') as processing,
                COUNT(*) FILTER (WHERE status = 'sent') as sent,
                COUNT(*) FILTER (WHERE status = 'delivered') as delivered,
                COUNT(*) FILTER (WHERE status = 'bounced') as bounced,
                COUNT(*) FILTER (WHERE status = 'failed') as failed,
                COUNT(*) FILTER (WHERE status = 'cancelled') as cancelled
            FROM scheduled_messages
            WHERE campaign_id = $1
            "#,
        )
        .bind(campaign_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(CampaignMessageCounts {
            pending: row.get::<Option<i64>, _>("pending").unwrap_or(0),
            processing: row.get::<Option<i64>, _>("processing").unwrap_or(0),
            sent: row.get::<Option<i64>, _>("sent").unwrap_or(0),
            delivered: row.get::<Option<i64>, _>("delivered").unwrap_or(0),
            bounced: row.get::<Option<i64>, _>("bounced").unwrap_or(0),
            failed: row.get::<Option<i64>, _>("failed").unwrap_or(0),
            cancelled: row.get::<Option<i64>, _>("cancelled").unwrap_or(0),
        })
    }
}

/// Campaign message counts by status
#[derive(Debug, Clone, Default)]
pub struct CampaignMessageCounts {
    pub pending: i64,
    pub processing: i64,
    pub sent: i64,
    pub delivered: i64,
    pub bounced: i64,
    pub failed: i64,
    pub cancelled: i64,
}

impl CampaignMessageCounts {
    pub fn total(&self) -> i64 {
        self.pending
            + self.processing
            + self.sent
            + self.delivered
            + self.bounced
            + self.failed
            + self.cancelled
    }

    pub fn completed(&self) -> i64 {
        self.sent + self.delivered + self.bounced + self.failed + self.cancelled
    }
}
