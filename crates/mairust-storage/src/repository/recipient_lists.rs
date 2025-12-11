//! Recipient list repository

use mairust_common::types::TenantId;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{CreateRecipientList, RecipientList, UpdateRecipientList};

/// Recipient list repository
#[derive(Clone)]
pub struct RecipientListRepository {
    pool: PgPool,
}

impl RecipientListRepository {
    /// Create a new recipient list repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new recipient list
    pub async fn create(&self, input: CreateRecipientList) -> Result<RecipientList, sqlx::Error> {
        let id = Uuid::new_v4();
        let metadata = input.metadata.unwrap_or_else(|| serde_json::json!({}));

        sqlx::query_as::<_, RecipientList>(
            r#"
            INSERT INTO recipient_lists (id, tenant_id, name, description, metadata)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&metadata)
        .fetch_one(&self.pool)
        .await
    }

    /// Get a recipient list by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<RecipientList>, sqlx::Error> {
        sqlx::query_as::<_, RecipientList>("SELECT * FROM recipient_lists WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Get a recipient list by ID and tenant
    pub async fn get_by_tenant(
        &self,
        tenant_id: TenantId,
        id: Uuid,
    ) -> Result<Option<RecipientList>, sqlx::Error> {
        sqlx::query_as::<_, RecipientList>(
            "SELECT * FROM recipient_lists WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// List recipient lists for a tenant
    pub async fn list_by_tenant(
        &self,
        tenant_id: TenantId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RecipientList>, sqlx::Error> {
        sqlx::query_as::<_, RecipientList>(
            r#"
            SELECT * FROM recipient_lists
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

    /// Update a recipient list
    pub async fn update(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        input: UpdateRecipientList,
    ) -> Result<Option<RecipientList>, sqlx::Error> {
        sqlx::query_as::<_, RecipientList>(
            r#"
            UPDATE recipient_lists SET
                name = COALESCE($3, name),
                description = COALESCE($4, description),
                metadata = COALESCE($5, metadata),
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.metadata)
        .fetch_optional(&self.pool)
        .await
    }

    /// Delete a recipient list
    pub async fn delete(&self, id: Uuid, tenant_id: TenantId) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM recipient_lists WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Count recipient lists by tenant
    pub async fn count_by_tenant(&self, tenant_id: TenantId) -> Result<i64, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM recipient_lists WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }
}
