//! Unsubscribe repository

use mairust_common::types::TenantId;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{CreateUnsubscribe, Unsubscribe};

/// Unsubscribe repository
#[derive(Clone)]
pub struct UnsubscribeRepository {
    pool: PgPool,
}

impl UnsubscribeRepository {
    /// Create a new unsubscribe repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new unsubscribe entry
    pub async fn create(&self, input: CreateUnsubscribe) -> Result<Unsubscribe, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query_as::<_, Unsubscribe>(
            r#"
            INSERT INTO unsubscribes (id, tenant_id, email, source, campaign_id, reason)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, email) DO UPDATE SET
                source = EXCLUDED.source,
                campaign_id = COALESCE(EXCLUDED.campaign_id, unsubscribes.campaign_id),
                reason = COALESCE(EXCLUDED.reason, unsubscribes.reason),
                unsubscribed_at = NOW()
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(&input.email)
        .bind(input.source.to_string())
        .bind(input.campaign_id)
        .bind(&input.reason)
        .fetch_one(&self.pool)
        .await
    }

    /// Check if an email is unsubscribed
    pub async fn is_unsubscribed(
        &self,
        tenant_id: TenantId,
        email: &str,
    ) -> Result<bool, sqlx::Error> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM unsubscribes WHERE tenant_id = $1 AND email = $2)",
        )
        .bind(tenant_id)
        .bind(email)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.0)
    }

    /// Get unsubscribe entry by email
    pub async fn get_by_email(
        &self,
        tenant_id: TenantId,
        email: &str,
    ) -> Result<Option<Unsubscribe>, sqlx::Error> {
        sqlx::query_as::<_, Unsubscribe>(
            "SELECT * FROM unsubscribes WHERE tenant_id = $1 AND email = $2",
        )
        .bind(tenant_id)
        .bind(email)
        .fetch_optional(&self.pool)
        .await
    }

    /// List unsubscribes for a tenant
    pub async fn list_by_tenant(
        &self,
        tenant_id: TenantId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Unsubscribe>, sqlx::Error> {
        sqlx::query_as::<_, Unsubscribe>(
            r#"
            SELECT * FROM unsubscribes
            WHERE tenant_id = $1
            ORDER BY unsubscribed_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// List unsubscribes by campaign
    pub async fn list_by_campaign(
        &self,
        campaign_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Unsubscribe>, sqlx::Error> {
        sqlx::query_as::<_, Unsubscribe>(
            r#"
            SELECT * FROM unsubscribes
            WHERE campaign_id = $1
            ORDER BY unsubscribed_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(campaign_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// Delete unsubscribe entry (re-subscribe)
    pub async fn delete(&self, tenant_id: TenantId, email: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM unsubscribes WHERE tenant_id = $1 AND email = $2",
        )
        .bind(tenant_id)
        .bind(email)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Count unsubscribes by tenant
    pub async fn count_by_tenant(&self, tenant_id: TenantId) -> Result<i64, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM unsubscribes WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }

    /// Count unsubscribes by campaign
    pub async fn count_by_campaign(&self, campaign_id: Uuid) -> Result<i64, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM unsubscribes WHERE campaign_id = $1",
        )
        .bind(campaign_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }

    /// Check multiple emails for unsubscribe status
    pub async fn filter_unsubscribed(
        &self,
        tenant_id: TenantId,
        emails: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        // Return list of emails that ARE unsubscribed
        let result: Vec<(String,)> = sqlx::query_as(
            "SELECT email FROM unsubscribes WHERE tenant_id = $1 AND email = ANY($2)",
        )
        .bind(tenant_id)
        .bind(emails)
        .fetch_all(&self.pool)
        .await?;

        Ok(result.into_iter().map(|(email,)| email).collect())
    }
}
