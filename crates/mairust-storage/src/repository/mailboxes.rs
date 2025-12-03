//! Mailbox repository

use crate::db::DatabasePool;
use crate::models::{CreateMailbox, Mailbox};
use async_trait::async_trait;
use mairust_common::types::{DomainId, MailboxId, TenantId, UserId};
use mairust_common::{Error, Result};
use uuid::Uuid;

/// Mailbox repository trait
#[async_trait]
pub trait MailboxRepository: Send + Sync {
    async fn create(&self, input: CreateMailbox) -> Result<Mailbox>;
    async fn get(&self, tenant_id: TenantId, id: MailboxId) -> Result<Option<Mailbox>>;
    async fn get_by_address(&self, address: &str) -> Result<Option<Mailbox>>;
    async fn list(&self, tenant_id: TenantId, limit: i64, offset: i64) -> Result<Vec<Mailbox>>;
    async fn list_by_domain(&self, domain_id: DomainId) -> Result<Vec<Mailbox>>;
    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<Mailbox>>;
    async fn update_quota(&self, id: MailboxId, quota_bytes: Option<i64>) -> Result<()>;
    async fn update_used_bytes(&self, id: MailboxId, delta: i64) -> Result<()>;
    async fn delete(&self, id: MailboxId) -> Result<()>;
}

/// Database mailbox repository
pub struct DbMailboxRepository {
    pool: DatabasePool,
}

impl DbMailboxRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MailboxRepository for DbMailboxRepository {
    async fn create(&self, input: CreateMailbox) -> Result<Mailbox> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            INSERT INTO mailboxes (id, tenant_id, domain_id, user_id, address, display_name, quota_bytes, used_bytes, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 0, $8, $9)
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(input.domain_id)
        .bind(input.user_id)
        .bind(&input.address)
        .bind(&input.display_name)
        .bind(input.quota_bytes)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(input.tenant_id, id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create mailbox".to_string()))
    }

    async fn get(&self, tenant_id: TenantId, id: MailboxId) -> Result<Option<Mailbox>> {
        sqlx::query_as::<_, Mailbox>("SELECT * FROM mailboxes WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn get_by_address(&self, address: &str) -> Result<Option<Mailbox>> {
        sqlx::query_as::<_, Mailbox>("SELECT * FROM mailboxes WHERE address = $1")
            .bind(address)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list(&self, tenant_id: TenantId, limit: i64, offset: i64) -> Result<Vec<Mailbox>> {
        sqlx::query_as::<_, Mailbox>(
            "SELECT * FROM mailboxes WHERE tenant_id = $1 ORDER BY address ASC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list_by_domain(&self, domain_id: DomainId) -> Result<Vec<Mailbox>> {
        sqlx::query_as::<_, Mailbox>(
            "SELECT * FROM mailboxes WHERE domain_id = $1 ORDER BY address ASC",
        )
        .bind(domain_id)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<Mailbox>> {
        sqlx::query_as::<_, Mailbox>(
            "SELECT * FROM mailboxes WHERE user_id = $1 ORDER BY address ASC",
        )
        .bind(user_id)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn update_quota(&self, id: MailboxId, quota_bytes: Option<i64>) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE mailboxes SET quota_bytes = $2, updated_at = $3 WHERE id = $1")
            .bind(id)
            .bind(quota_bytes)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn update_used_bytes(&self, id: MailboxId, delta: i64) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query(
            "UPDATE mailboxes SET used_bytes = used_bytes + $2, updated_at = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(delta)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: MailboxId) -> Result<()> {
        sqlx::query("DELETE FROM mailboxes WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
