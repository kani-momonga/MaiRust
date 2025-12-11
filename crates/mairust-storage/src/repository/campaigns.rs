//! Campaign repository

use chrono::Utc;
use mairust_common::types::TenantId;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{Campaign, CampaignStatus, CreateCampaign, UpdateCampaign};

/// Campaign repository
#[derive(Clone)]
pub struct CampaignRepository {
    pool: PgPool,
}

impl CampaignRepository {
    /// Create a new campaign repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new campaign
    pub async fn create(&self, input: CreateCampaign) -> Result<Campaign, sqlx::Error> {
        let id = Uuid::new_v4();
        let tags = serde_json::to_value(input.tags.unwrap_or_default()).unwrap_or_default();
        let metadata = input.metadata.unwrap_or_else(|| serde_json::json!({}));

        sqlx::query_as::<_, Campaign>(
            r#"
            INSERT INTO campaigns (
                id, tenant_id, name, description, subject, from_address, from_name,
                reply_to, html_body, text_body, recipient_list_id, scheduled_at,
                rate_limit_per_hour, rate_limit_per_minute, tags, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.subject)
        .bind(&input.from_address)
        .bind(&input.from_name)
        .bind(&input.reply_to)
        .bind(&input.html_body)
        .bind(&input.text_body)
        .bind(input.recipient_list_id)
        .bind(input.scheduled_at)
        .bind(input.rate_limit_per_hour.unwrap_or(5000))
        .bind(input.rate_limit_per_minute.unwrap_or(100))
        .bind(&tags)
        .bind(&metadata)
        .fetch_one(&self.pool)
        .await
    }

    /// Get a campaign by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<Campaign>, sqlx::Error> {
        sqlx::query_as::<_, Campaign>("SELECT * FROM campaigns WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Get a campaign by ID and tenant
    pub async fn get_by_tenant(
        &self,
        tenant_id: TenantId,
        id: Uuid,
    ) -> Result<Option<Campaign>, sqlx::Error> {
        sqlx::query_as::<_, Campaign>(
            "SELECT * FROM campaigns WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// List campaigns for a tenant
    pub async fn list_by_tenant(
        &self,
        tenant_id: TenantId,
        status: Option<CampaignStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Campaign>, sqlx::Error> {
        if let Some(status) = status {
            sqlx::query_as::<_, Campaign>(
                r#"
                SELECT * FROM campaigns
                WHERE tenant_id = $1 AND status = $2
                ORDER BY created_at DESC
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
            sqlx::query_as::<_, Campaign>(
                r#"
                SELECT * FROM campaigns
                WHERE tenant_id = $1
                ORDER BY created_at DESC
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

    /// Update a campaign
    pub async fn update(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        input: UpdateCampaign,
    ) -> Result<Option<Campaign>, sqlx::Error> {
        // Get current campaign first
        let current = match self.get_by_tenant(tenant_id, id).await? {
            Some(c) => c,
            None => return Ok(None),
        };

        // Only allow updates to draft campaigns
        if current.status != "draft" {
            return Ok(Some(current)); // Return unchanged
        }

        let tags = input
            .tags
            .map(|t| serde_json::to_value(t).unwrap_or_default())
            .unwrap_or(current.tags);

        sqlx::query_as::<_, Campaign>(
            r#"
            UPDATE campaigns SET
                name = COALESCE($3, name),
                description = COALESCE($4, description),
                subject = COALESCE($5, subject),
                from_address = COALESCE($6, from_address),
                from_name = COALESCE($7, from_name),
                reply_to = COALESCE($8, reply_to),
                html_body = COALESCE($9, html_body),
                text_body = COALESCE($10, text_body),
                recipient_list_id = COALESCE($11, recipient_list_id),
                scheduled_at = COALESCE($12, scheduled_at),
                rate_limit_per_hour = COALESCE($13, rate_limit_per_hour),
                rate_limit_per_minute = COALESCE($14, rate_limit_per_minute),
                tags = $15,
                metadata = COALESCE($16, metadata),
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.subject)
        .bind(&input.from_address)
        .bind(&input.from_name)
        .bind(&input.reply_to)
        .bind(&input.html_body)
        .bind(&input.text_body)
        .bind(input.recipient_list_id)
        .bind(input.scheduled_at)
        .bind(input.rate_limit_per_hour)
        .bind(input.rate_limit_per_minute)
        .bind(&tags)
        .bind(&input.metadata)
        .fetch_optional(&self.pool)
        .await
    }

    /// Update campaign status
    pub async fn update_status(
        &self,
        id: Uuid,
        status: CampaignStatus,
    ) -> Result<Option<Campaign>, sqlx::Error> {
        let started_at = if status == CampaignStatus::Sending {
            Some(Utc::now())
        } else {
            None
        };

        let completed_at = if matches!(
            status,
            CampaignStatus::Completed | CampaignStatus::Cancelled | CampaignStatus::Failed
        ) {
            Some(Utc::now())
        } else {
            None
        };

        sqlx::query_as::<_, Campaign>(
            r#"
            UPDATE campaigns SET
                status = $2,
                started_at = COALESCE($3, started_at),
                completed_at = COALESCE($4, completed_at),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(status.to_string())
        .bind(started_at)
        .bind(completed_at)
        .fetch_optional(&self.pool)
        .await
    }

    /// Set total recipients count
    pub async fn set_total_recipients(
        &self,
        id: Uuid,
        total: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE campaigns SET
                total_recipients = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(total)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a campaign
    pub async fn delete(&self, id: Uuid, tenant_id: TenantId) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM campaigns WHERE id = $1 AND tenant_id = $2 AND status = 'draft'",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get campaigns ready to start (scheduled time has passed)
    pub async fn get_scheduled_ready(&self) -> Result<Vec<Campaign>, sqlx::Error> {
        sqlx::query_as::<_, Campaign>(
            r#"
            SELECT * FROM campaigns
            WHERE status = 'scheduled'
              AND scheduled_at IS NOT NULL
              AND scheduled_at <= NOW()
            ORDER BY scheduled_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Count campaigns by tenant
    pub async fn count_by_tenant(
        &self,
        tenant_id: TenantId,
        status: Option<CampaignStatus>,
    ) -> Result<i64, sqlx::Error> {
        let count: (i64,) = if let Some(status) = status {
            sqlx::query_as(
                "SELECT COUNT(*) FROM campaigns WHERE tenant_id = $1 AND status = $2",
            )
            .bind(tenant_id)
            .bind(status.to_string())
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM campaigns WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(count.0)
    }
}
