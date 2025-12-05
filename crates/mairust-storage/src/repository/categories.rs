//! Category repository
//!
//! Database operations for AI categorization.

use crate::db::DatabasePool;
use crate::models::{Category, CreateCategory};
use anyhow::Result;
use mairust_common::types::{MessageId, TenantId};
use uuid::Uuid;

/// Category repository
pub struct CategoryRepository {
    pool: DatabasePool,
}

impl CategoryRepository {
    /// Create a new category repository
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Create a new category
    pub async fn create(&self, input: CreateCategory) -> Result<Category> {
        let category_id = Uuid::new_v4();

        let category = sqlx::query_as::<_, Category>(
            r#"
            INSERT INTO categories (id, tenant_id, name, description, color, priority, auto_rules, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(category_id)
        .bind(input.tenant_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.color)
        .bind(input.priority.unwrap_or(0))
        .bind(input.auto_rules.unwrap_or(serde_json::json!({})))
        .fetch_one(self.pool.pool())
        .await?;

        Ok(category)
    }

    /// Get a category by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<Category>> {
        let category = sqlx::query_as::<_, Category>("SELECT * FROM categories WHERE id = $1")
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await?;

        Ok(category)
    }

    /// Get a category by name for a tenant
    pub async fn get_by_name(&self, tenant_id: TenantId, name: &str) -> Result<Option<Category>> {
        let category = sqlx::query_as::<_, Category>(
            "SELECT * FROM categories WHERE tenant_id = $1 AND name = $2",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(self.pool.pool())
        .await?;

        Ok(category)
    }

    /// List all categories for a tenant
    pub async fn list(&self, tenant_id: TenantId) -> Result<Vec<Category>> {
        let categories = sqlx::query_as::<_, Category>(
            "SELECT * FROM categories WHERE tenant_id = $1 ORDER BY priority DESC, name ASC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(categories)
    }

    /// Update a category
    pub async fn update(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        color: Option<&str>,
        priority: Option<i32>,
        auto_rules: Option<serde_json::Value>,
    ) -> Result<Option<Category>> {
        let category = sqlx::query_as::<_, Category>(
            r#"
            UPDATE categories SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                color = COALESCE($4, color),
                priority = COALESCE($5, priority),
                auto_rules = COALESCE($6, auto_rules),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(color)
        .bind(priority)
        .bind(auto_rules)
        .fetch_optional(self.pool.pool())
        .await?;

        Ok(category)
    }

    /// Delete a category
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM categories WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Assign a category to a message with AI metadata
    pub async fn assign_to_message(
        &self,
        message_id: MessageId,
        category_id: Uuid,
        confidence: f32,
        summary: Option<&str>,
        metadata: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE messages SET
                category_id = $2,
                category_confidence = $3,
                ai_summary = $4,
                ai_metadata = $5
            WHERE id = $1
            "#,
        )
        .bind(message_id)
        .bind(category_id)
        .bind(confidence)
        .bind(summary)
        .bind(metadata)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Get messages in a category
    pub async fn get_messages(
        &self,
        category_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MessageId>> {
        let messages: Vec<(MessageId,)> = sqlx::query_as(
            r#"
            SELECT id FROM messages
            WHERE category_id = $1
            ORDER BY received_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(category_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(messages.into_iter().map(|(id,)| id).collect())
    }

    /// Get category statistics for a tenant
    pub async fn get_stats(&self, tenant_id: TenantId) -> Result<Vec<(Uuid, String, i64)>> {
        let stats: Vec<(Uuid, String, i64)> = sqlx::query_as(
            r#"
            SELECT c.id, c.name, COUNT(m.id) as message_count
            FROM categories c
            LEFT JOIN messages m ON m.category_id = c.id
            WHERE c.tenant_id = $1
            GROUP BY c.id, c.name
            ORDER BY c.priority DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(stats)
    }
}
