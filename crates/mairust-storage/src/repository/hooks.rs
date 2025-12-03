//! Hook repository

use crate::db::DatabasePool;
use crate::models::Hook;
use async_trait::async_trait;
use mairust_common::types::{HookId, HookType, TenantId};
use mairust_common::{Error, Result};
use uuid::Uuid;

/// Hook repository trait
#[async_trait]
pub trait HookRepository: Send + Sync {
    async fn create(&self, input: CreateHook) -> Result<Hook>;
    async fn get(&self, id: HookId) -> Result<Option<Hook>>;
    async fn list(&self, tenant_id: Option<TenantId>) -> Result<Vec<Hook>>;
    async fn list_by_type(&self, hook_type: HookType) -> Result<Vec<Hook>>;
    async fn list_enabled_by_type(&self, hook_type: HookType) -> Result<Vec<Hook>>;
    async fn enable(&self, id: HookId) -> Result<()>;
    async fn disable(&self, id: HookId) -> Result<()>;
    async fn delete(&self, id: HookId) -> Result<()>;
}

/// Create hook input
#[derive(Debug, Clone)]
pub struct CreateHook {
    pub tenant_id: Option<TenantId>,
    pub name: String,
    pub hook_type: HookType,
    pub plugin_id: String,
    pub priority: i32,
    pub timeout_ms: i32,
    pub on_timeout: String,
    pub on_error: String,
    pub filter_config: serde_json::Value,
    pub config: serde_json::Value,
}

/// Database hook repository
pub struct DbHookRepository {
    pool: DatabasePool,
}

impl DbHookRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HookRepository for DbHookRepository {
    async fn create(&self, input: CreateHook) -> Result<Hook> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();
        let hook_type_str = input.hook_type.to_string();

        sqlx::query(
            r#"
            INSERT INTO hooks (
                id, tenant_id, name, hook_type, plugin_id, enabled, priority,
                timeout_ms, on_timeout, on_error, filter_config, config, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, true, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(&input.name)
        .bind(&hook_type_str)
        .bind(&input.plugin_id)
        .bind(input.priority)
        .bind(input.timeout_ms)
        .bind(&input.on_timeout)
        .bind(&input.on_error)
        .bind(&input.filter_config)
        .bind(&input.config)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create hook".to_string()))
    }

    async fn get(&self, id: HookId) -> Result<Option<Hook>> {
        sqlx::query_as::<_, Hook>("SELECT * FROM hooks WHERE id = $1")
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list(&self, tenant_id: Option<TenantId>) -> Result<Vec<Hook>> {
        match tenant_id {
            Some(tid) => sqlx::query_as::<_, Hook>(
                "SELECT * FROM hooks WHERE tenant_id = $1 OR tenant_id IS NULL ORDER BY priority ASC",
            )
            .bind(tid)
            .fetch_all(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string())),
            None => sqlx::query_as::<_, Hook>(
                "SELECT * FROM hooks ORDER BY priority ASC",
            )
            .fetch_all(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string())),
        }
    }

    async fn list_by_type(&self, hook_type: HookType) -> Result<Vec<Hook>> {
        let hook_type_str = hook_type.to_string();
        sqlx::query_as::<_, Hook>(
            "SELECT * FROM hooks WHERE hook_type = $1 ORDER BY priority ASC",
        )
        .bind(hook_type_str)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list_enabled_by_type(&self, hook_type: HookType) -> Result<Vec<Hook>> {
        let hook_type_str = hook_type.to_string();
        sqlx::query_as::<_, Hook>(
            "SELECT * FROM hooks WHERE hook_type = $1 AND enabled = true ORDER BY priority ASC",
        )
        .bind(hook_type_str)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn enable(&self, id: HookId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE hooks SET enabled = true, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn disable(&self, id: HookId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE hooks SET enabled = false, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: HookId) -> Result<()> {
        sqlx::query("DELETE FROM hooks WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
