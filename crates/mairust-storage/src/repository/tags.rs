//! Tag repository
//!
//! Database operations for message tagging.

use crate::db::DatabasePool;
use crate::models::{CreateTag, MessageTag, Tag, UpdateTag};
use anyhow::Result;
use mairust_common::types::{MessageId, TenantId};
use uuid::Uuid;

/// Tag repository
pub struct TagRepository {
    pool: DatabasePool,
}

impl TagRepository {
    /// Create a new tag repository
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Create a new tag
    pub async fn create(&self, input: CreateTag) -> Result<Tag> {
        let tag_id = Uuid::new_v4();

        let tag = sqlx::query_as::<_, Tag>(
            r#"
            INSERT INTO tags (id, tenant_id, name, color, description, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(tag_id)
        .bind(input.tenant_id)
        .bind(&input.name)
        .bind(&input.color)
        .bind(&input.description)
        .fetch_one(self.pool.pool())
        .await?;

        Ok(tag)
    }

    /// Get a tag by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<Tag>> {
        let tag = sqlx::query_as::<_, Tag>("SELECT * FROM tags WHERE id = $1")
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await?;

        Ok(tag)
    }

    /// Get a tag by name for a tenant
    pub async fn get_by_name(&self, tenant_id: TenantId, name: &str) -> Result<Option<Tag>> {
        let tag = sqlx::query_as::<_, Tag>(
            "SELECT * FROM tags WHERE tenant_id = $1 AND name = $2",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(self.pool.pool())
        .await?;

        Ok(tag)
    }

    /// List all tags for a tenant
    pub async fn list(&self, tenant_id: TenantId) -> Result<Vec<Tag>> {
        let tags = sqlx::query_as::<_, Tag>(
            "SELECT * FROM tags WHERE tenant_id = $1 ORDER BY name ASC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(tags)
    }

    /// Update a tag
    pub async fn update(&self, id: Uuid, input: UpdateTag) -> Result<Option<Tag>> {
        let tag = sqlx::query_as::<_, Tag>(
            r#"
            UPDATE tags SET
                name = COALESCE($2, name),
                color = COALESCE($3, color),
                description = COALESCE($4, description),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.color)
        .bind(&input.description)
        .fetch_optional(self.pool.pool())
        .await?;

        Ok(tag)
    }

    /// Delete a tag
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM tags WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Add a tag to a message
    pub async fn add_to_message(&self, message_id: MessageId, tag_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO message_tags (message_id, tag_id, created_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (message_id, tag_id) DO NOTHING
            "#,
        )
        .bind(message_id)
        .bind(tag_id)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Remove a tag from a message
    pub async fn remove_from_message(&self, message_id: MessageId, tag_id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM message_tags WHERE message_id = $1 AND tag_id = $2",
        )
        .bind(message_id)
        .bind(tag_id)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get tags for a message
    pub async fn get_message_tags(&self, message_id: MessageId) -> Result<Vec<Tag>> {
        let tags = sqlx::query_as::<_, Tag>(
            r#"
            SELECT t.* FROM tags t
            INNER JOIN message_tags mt ON mt.tag_id = t.id
            WHERE mt.message_id = $1
            ORDER BY t.name ASC
            "#,
        )
        .bind(message_id)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(tags)
    }

    /// Get messages with a specific tag
    pub async fn get_tagged_messages(
        &self,
        tag_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MessageId>> {
        let messages: Vec<(MessageId,)> = sqlx::query_as(
            r#"
            SELECT message_id FROM message_tags
            WHERE tag_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tag_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(messages.into_iter().map(|(id,)| id).collect())
    }

    /// Set tags for a message (replace all existing tags)
    pub async fn set_message_tags(&self, message_id: MessageId, tag_ids: &[Uuid]) -> Result<()> {
        let pool = self.pool.pool();

        // Remove existing tags
        sqlx::query("DELETE FROM message_tags WHERE message_id = $1")
            .bind(message_id)
            .execute(pool)
            .await?;

        // Add new tags
        for tag_id in tag_ids {
            sqlx::query(
                r#"
                INSERT INTO message_tags (message_id, tag_id, created_at)
                VALUES ($1, $2, NOW())
                "#,
            )
            .bind(message_id)
            .bind(tag_id)
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    /// Get or create a tag by name
    pub async fn get_or_create(&self, tenant_id: TenantId, name: &str) -> Result<Tag> {
        // Try to get existing tag
        if let Some(tag) = self.get_by_name(tenant_id, name).await? {
            return Ok(tag);
        }

        // Create new tag
        let input = CreateTag {
            tenant_id,
            name: name.to_string(),
            color: None,
            description: None,
        };

        self.create(input).await
    }
}
