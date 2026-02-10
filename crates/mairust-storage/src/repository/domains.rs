//! Domain repository

use crate::db::DatabasePool;
use crate::models::{CreateDomain, Domain};
use async_trait::async_trait;
use mairust_common::types::{DomainId, TenantId};
use mairust_common::{Error, Result};
use uuid::Uuid;

/// Domain repository trait
#[async_trait]
pub trait DomainRepository: Send + Sync {
    async fn create(&self, input: CreateDomain) -> Result<Domain>;
    async fn get(&self, tenant_id: TenantId, id: DomainId) -> Result<Option<Domain>>;
    async fn get_by_name(&self, name: &str) -> Result<Option<Domain>>;
    async fn list(&self, tenant_id: TenantId) -> Result<Vec<Domain>>;
    async fn verify(&self, id: DomainId) -> Result<()>;
    async fn set_dkim(&self, id: DomainId, selector: String, private_key: String) -> Result<()>;
    async fn delete(&self, id: DomainId) -> Result<()>;
}

/// Database domain repository
pub struct DbDomainRepository {
    pool: DatabasePool,
}

impl DbDomainRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Find domain by name (for SMTP handler - cross-tenant lookup required for mail routing)
    pub async fn find_by_name(&self, name: &str) -> Result<Option<Domain>> {
        sqlx::query_as::<_, Domain>("SELECT * FROM domains WHERE name = $1")
            .bind(name)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    /// Find domain by name within a specific tenant (for API handlers)
    pub async fn find_by_name_for_tenant(
        &self,
        tenant_id: TenantId,
        name: &str,
    ) -> Result<Option<Domain>> {
        sqlx::query_as::<_, Domain>(
            "SELECT * FROM domains WHERE tenant_id = $1 AND name = $2",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }
}

#[async_trait]
impl DomainRepository for DbDomainRepository {
    async fn create(&self, input: CreateDomain) -> Result<Domain> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            INSERT INTO domains (id, tenant_id, name, verified, created_at, updated_at)
            VALUES ($1, $2, $3, false, $4, $5)
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(&input.name)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(input.tenant_id, id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create domain".to_string()))
    }

    async fn get(&self, tenant_id: TenantId, id: DomainId) -> Result<Option<Domain>> {
        sqlx::query_as::<_, Domain>("SELECT * FROM domains WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<Domain>> {
        sqlx::query_as::<_, Domain>("SELECT * FROM domains WHERE name = $1")
            .bind(name)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list(&self, tenant_id: TenantId) -> Result<Vec<Domain>> {
        sqlx::query_as::<_, Domain>(
            "SELECT * FROM domains WHERE tenant_id = $1 ORDER BY name ASC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn verify(&self, id: DomainId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE domains SET verified = true, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn set_dkim(&self, id: DomainId, selector: String, private_key: String) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query(
            "UPDATE domains SET dkim_selector = $2, dkim_private_key = $3, updated_at = $4 WHERE id = $1",
        )
        .bind(id)
        .bind(selector)
        .bind(private_key)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: DomainId) -> Result<()> {
        sqlx::query("DELETE FROM domains WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
