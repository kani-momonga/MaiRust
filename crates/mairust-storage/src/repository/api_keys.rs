//! API Key repository

use crate::db::DatabasePool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mairust_common::types::{TenantId, UserId};
use mairust_common::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// API Key ID type
pub type ApiKeyId = Uuid;

/// API Key model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: ApiKeyId,
    pub tenant_id: TenantId,
    pub user_id: Option<UserId>,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub scopes: serde_json::Value,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    /// Check if the API key has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            expires_at < Utc::now()
        } else {
            false
        }
    }

    /// Get scopes as a vector
    pub fn scopes_vec(&self) -> Vec<String> {
        serde_json::from_value(self.scopes.clone()).unwrap_or_default()
    }

    /// Check if the API key has a specific scope
    pub fn has_scope(&self, scope: &str) -> bool {
        let scopes = self.scopes_vec();
        scopes.contains(&"*".to_string()) || scopes.contains(&scope.to_string())
    }
}

/// API key repository trait
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    /// Find an API key by its prefix (for initial lookup)
    async fn find_by_prefix(&self, prefix: &str) -> Result<Vec<ApiKey>>;

    /// Get an API key by ID
    async fn get(&self, id: ApiKeyId) -> Result<Option<ApiKey>>;

    /// Update last_used_at timestamp
    async fn update_last_used(&self, id: ApiKeyId) -> Result<()>;
}

/// Database API key repository
pub struct DbApiKeyRepository {
    pool: DatabasePool,
}

impl DbApiKeyRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyRepository for DbApiKeyRepository {
    async fn find_by_prefix(&self, prefix: &str) -> Result<Vec<ApiKey>> {
        sqlx::query_as::<_, ApiKey>(
            r#"
            SELECT id, tenant_id, user_id, name, key_hash, key_prefix, scopes,
                   expires_at, last_used_at, created_at
            FROM api_keys
            WHERE key_prefix = $1
              AND (expires_at IS NULL OR expires_at > NOW())
            LIMIT 10
            "#,
        )
        .bind(prefix)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn get(&self, id: ApiKeyId) -> Result<Option<ApiKey>> {
        sqlx::query_as::<_, ApiKey>(
            r#"
            SELECT id, tenant_id, user_id, name, key_hash, key_prefix, scopes,
                   expires_at, last_used_at, created_at
            FROM api_keys
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn update_last_used(&self, id: ApiKeyId) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE api_keys SET last_used_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
