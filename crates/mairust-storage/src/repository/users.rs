//! User repository

use crate::db::DatabasePool;
use crate::models::{CreateUser, User};
use async_trait::async_trait;
use mairust_common::types::{TenantId, UserId};
use mairust_common::{Error, Result};
use uuid::Uuid;

/// User repository trait
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, input: CreateUser, password_hash: String) -> Result<User>;
    async fn get(&self, tenant_id: TenantId, id: UserId) -> Result<Option<User>>;
    async fn get_by_email(&self, email: &str) -> Result<Option<User>>;
    async fn list(&self, tenant_id: TenantId, limit: i64, offset: i64) -> Result<Vec<User>>;
    async fn update_password(&self, id: UserId, password_hash: String) -> Result<()>;
    async fn deactivate(&self, id: UserId) -> Result<()>;
    async fn activate(&self, id: UserId) -> Result<()>;
}

/// Database user repository
pub struct DbUserRepository {
    pool: DatabasePool,
}

impl DbUserRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for DbUserRepository {
    async fn create(&self, input: CreateUser, password_hash: String) -> Result<User> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();
        let role = format!("{:?}", input.role).to_lowercase();

        sqlx::query(
            r#"
            INSERT INTO users (id, tenant_id, email, password_hash, name, role, active, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, true, $7, $8)
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(&input.email)
        .bind(&password_hash)
        .bind(&input.name)
        .bind(&role)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(input.tenant_id, id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create user".to_string()))
    }

    async fn get(&self, tenant_id: TenantId, id: UserId) -> Result<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn get_by_email(&self, email: &str) -> Result<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1 AND active = true")
            .bind(email)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list(&self, tenant_id: TenantId, limit: i64, offset: i64) -> Result<Vec<User>> {
        sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn update_password(&self, id: UserId, password_hash: String) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE users SET password_hash = $2, updated_at = $3 WHERE id = $1")
            .bind(id)
            .bind(password_hash)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn deactivate(&self, id: UserId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE users SET active = false, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn activate(&self, id: UserId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE users SET active = true, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
