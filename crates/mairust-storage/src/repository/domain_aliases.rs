//! Domain alias repository

use crate::db::DatabasePool;
use crate::models::{CreateDomainAlias, DomainAlias};
use async_trait::async_trait;
use mairust_common::types::{DomainAliasId, DomainId, TenantId};
use mairust_common::{Error, Result};
use uuid::Uuid;

/// Domain alias repository trait
#[async_trait]
pub trait DomainAliasRepository: Send + Sync {
    async fn create(&self, input: CreateDomainAlias) -> Result<DomainAlias>;
    async fn get(&self, tenant_id: TenantId, id: DomainAliasId) -> Result<Option<DomainAlias>>;
    async fn get_by_alias_domain(&self, alias_domain: &str) -> Result<Option<DomainAlias>>;
    async fn list(&self, tenant_id: TenantId) -> Result<Vec<DomainAlias>>;
    async fn list_by_primary_domain(&self, primary_domain_id: DomainId) -> Result<Vec<DomainAlias>>;
    async fn enable(&self, id: DomainAliasId) -> Result<()>;
    async fn disable(&self, id: DomainAliasId) -> Result<()>;
    async fn delete(&self, id: DomainAliasId) -> Result<()>;
}

/// Database domain alias repository
pub struct DbDomainAliasRepository {
    pool: DatabasePool,
}

impl DbDomainAliasRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DomainAliasRepository for DbDomainAliasRepository {
    async fn create(&self, input: CreateDomainAlias) -> Result<DomainAlias> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            INSERT INTO domain_aliases (id, tenant_id, alias_domain, primary_domain_id, enabled, created_at, updated_at)
            VALUES ($1, $2, $3, $4, true, $5, $6)
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(&input.alias_domain)
        .bind(input.primary_domain_id)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(input.tenant_id, id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create domain alias".to_string()))
    }

    async fn get(&self, tenant_id: TenantId, id: DomainAliasId) -> Result<Option<DomainAlias>> {
        sqlx::query_as::<_, DomainAlias>(
            "SELECT * FROM domain_aliases WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn get_by_alias_domain(&self, alias_domain: &str) -> Result<Option<DomainAlias>> {
        sqlx::query_as::<_, DomainAlias>(
            "SELECT * FROM domain_aliases WHERE alias_domain = $1 AND enabled = true",
        )
        .bind(alias_domain)
        .fetch_optional(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list(&self, tenant_id: TenantId) -> Result<Vec<DomainAlias>> {
        sqlx::query_as::<_, DomainAlias>(
            "SELECT * FROM domain_aliases WHERE tenant_id = $1 ORDER BY alias_domain ASC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list_by_primary_domain(&self, primary_domain_id: DomainId) -> Result<Vec<DomainAlias>> {
        sqlx::query_as::<_, DomainAlias>(
            "SELECT * FROM domain_aliases WHERE primary_domain_id = $1 ORDER BY alias_domain ASC",
        )
        .bind(primary_domain_id)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn enable(&self, id: DomainAliasId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE domain_aliases SET enabled = true, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn disable(&self, id: DomainAliasId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE domain_aliases SET enabled = false, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: DomainAliasId) -> Result<()> {
        sqlx::query("DELETE FROM domain_aliases WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
