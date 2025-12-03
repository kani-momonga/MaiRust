//! Tenant repository

use crate::db::DatabasePool;
use crate::models::{CreateTenant, Tenant};
use async_trait::async_trait;
use mairust_common::types::TenantId;
use mairust_common::{Error, Result};
use uuid::Uuid;

/// Tenant repository trait
#[async_trait]
pub trait TenantRepository: Send + Sync {
    async fn create(&self, input: CreateTenant) -> Result<Tenant>;
    async fn get(&self, id: TenantId) -> Result<Option<Tenant>>;
    async fn get_by_slug(&self, slug: &str) -> Result<Option<Tenant>>;
    async fn list(&self, limit: i64, offset: i64) -> Result<Vec<Tenant>>;
    async fn update(&self, id: TenantId, name: Option<String>, plan: Option<String>) -> Result<()>;
    async fn delete(&self, id: TenantId) -> Result<()>;
}

/// Database tenant repository
pub struct DbTenantRepository {
    pool: DatabasePool,
}

impl DbTenantRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenantRepository for DbTenantRepository {
    async fn create(&self, input: CreateTenant) -> Result<Tenant> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();
        let settings = input.settings.unwrap_or(serde_json::json!({}));
        let plan = input.plan.unwrap_or_else(|| "free".to_string());

        sqlx::query(
            r#"
            INSERT INTO tenants (id, name, slug, status, plan, settings, created_at, updated_at)
            VALUES ($1, $2, $3, 'active', $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&plan)
        .bind(&settings)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create tenant".to_string()))
    }

    async fn get(&self, id: TenantId) -> Result<Option<Tenant>> {
        sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = $1 AND status != 'deleted'")
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn get_by_slug(&self, slug: &str) -> Result<Option<Tenant>> {
        sqlx::query_as::<_, Tenant>(
            "SELECT * FROM tenants WHERE slug = $1 AND status != 'deleted'",
        )
        .bind(slug)
        .fetch_optional(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list(&self, limit: i64, offset: i64) -> Result<Vec<Tenant>> {
        sqlx::query_as::<_, Tenant>(
            "SELECT * FROM tenants WHERE status != 'deleted' ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn update(&self, id: TenantId, name: Option<String>, plan: Option<String>) -> Result<()> {
        let now = chrono::Utc::now();

        if let Some(name) = name {
            sqlx::query("UPDATE tenants SET name = $2, updated_at = $3 WHERE id = $1")
                .bind(id)
                .bind(name)
                .bind(now)
                .execute(self.pool.pool())
                .await
                .map_err(|e| Error::Database(e.to_string()))?;
        }

        if let Some(plan) = plan {
            sqlx::query("UPDATE tenants SET plan = $2, updated_at = $3 WHERE id = $1")
                .bind(id)
                .bind(plan)
                .bind(now)
                .execute(self.pool.pool())
                .await
                .map_err(|e| Error::Database(e.to_string()))?;
        }

        Ok(())
    }

    async fn delete(&self, id: TenantId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE tenants SET status = 'deleted', updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
