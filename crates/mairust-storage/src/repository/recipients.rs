//! Recipient repository

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{CreateRecipient, Recipient, RecipientStatus, UpdateRecipient};

/// Recipient repository
#[derive(Clone)]
pub struct RecipientRepository {
    pool: PgPool,
}

impl RecipientRepository {
    /// Create a new recipient repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new recipient
    pub async fn create(&self, input: CreateRecipient) -> Result<Recipient, sqlx::Error> {
        let id = Uuid::new_v4();
        let attributes = input.attributes.unwrap_or_else(|| serde_json::json!({}));

        sqlx::query_as::<_, Recipient>(
            r#"
            INSERT INTO recipients (id, recipient_list_id, email, name, attributes)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(input.recipient_list_id)
        .bind(&input.email)
        .bind(&input.name)
        .bind(&attributes)
        .fetch_one(&self.pool)
        .await
    }

    /// Create multiple recipients in batch
    pub async fn create_batch(
        &self,
        recipient_list_id: Uuid,
        recipients: Vec<(String, Option<String>, Option<serde_json::Value>)>,
    ) -> Result<u64, sqlx::Error> {
        let mut count = 0u64;

        // Use a transaction for batch insert
        let mut tx = self.pool.begin().await?;

        for (email, name, attributes) in recipients {
            let id = Uuid::new_v4();
            let attrs = attributes.unwrap_or_else(|| serde_json::json!({}));

            let result = sqlx::query(
                r#"
                INSERT INTO recipients (id, recipient_list_id, email, name, attributes)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (recipient_list_id, email) DO NOTHING
                "#,
            )
            .bind(id)
            .bind(recipient_list_id)
            .bind(&email)
            .bind(&name)
            .bind(&attrs)
            .execute(&mut *tx)
            .await?;

            count += result.rows_affected();
        }

        tx.commit().await?;
        Ok(count)
    }

    /// Get a recipient by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<Recipient>, sqlx::Error> {
        sqlx::query_as::<_, Recipient>("SELECT * FROM recipients WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Get a recipient by list and email
    pub async fn get_by_email(
        &self,
        recipient_list_id: Uuid,
        email: &str,
    ) -> Result<Option<Recipient>, sqlx::Error> {
        sqlx::query_as::<_, Recipient>(
            "SELECT * FROM recipients WHERE recipient_list_id = $1 AND email = $2",
        )
        .bind(recipient_list_id)
        .bind(email)
        .fetch_optional(&self.pool)
        .await
    }

    /// List recipients for a recipient list
    pub async fn list_by_list(
        &self,
        recipient_list_id: Uuid,
        status: Option<RecipientStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Recipient>, sqlx::Error> {
        if let Some(status) = status {
            sqlx::query_as::<_, Recipient>(
                r#"
                SELECT * FROM recipients
                WHERE recipient_list_id = $1 AND status = $2
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(recipient_list_id)
            .bind(status.to_string())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, Recipient>(
                r#"
                SELECT * FROM recipients
                WHERE recipient_list_id = $1
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(recipient_list_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
    }

    /// List active recipients for a recipient list (for sending)
    pub async fn list_active_by_list(
        &self,
        recipient_list_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Recipient>, sqlx::Error> {
        sqlx::query_as::<_, Recipient>(
            r#"
            SELECT * FROM recipients
            WHERE recipient_list_id = $1 AND status = 'active'
            ORDER BY id ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(recipient_list_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// Update a recipient
    pub async fn update(
        &self,
        id: Uuid,
        input: UpdateRecipient,
    ) -> Result<Option<Recipient>, sqlx::Error> {
        let unsubscribed_at = if input.status == Some(RecipientStatus::Unsubscribed) {
            Some(Utc::now())
        } else {
            None
        };

        sqlx::query_as::<_, Recipient>(
            r#"
            UPDATE recipients SET
                name = COALESCE($2, name),
                status = COALESCE($3, status),
                attributes = COALESCE($4, attributes),
                unsubscribed_at = COALESCE($5, unsubscribed_at),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(&input.name)
        .bind(input.status.map(|s| s.to_string()))
        .bind(&input.attributes)
        .bind(unsubscribed_at)
        .fetch_optional(&self.pool)
        .await
    }

    /// Update recipient status
    pub async fn update_status(
        &self,
        id: Uuid,
        status: RecipientStatus,
    ) -> Result<Option<Recipient>, sqlx::Error> {
        let unsubscribed_at = if status == RecipientStatus::Unsubscribed {
            Some(Utc::now())
        } else {
            None
        };

        sqlx::query_as::<_, Recipient>(
            r#"
            UPDATE recipients SET
                status = $2,
                unsubscribed_at = COALESCE($3, unsubscribed_at),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(status.to_string())
        .bind(unsubscribed_at)
        .fetch_optional(&self.pool)
        .await
    }

    /// Update recipient status by email (for bounce handling)
    pub async fn update_status_by_email(
        &self,
        recipient_list_id: Uuid,
        email: &str,
        status: RecipientStatus,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE recipients SET
                status = $3,
                updated_at = NOW()
            WHERE recipient_list_id = $1 AND email = $2
            "#,
        )
        .bind(recipient_list_id)
        .bind(email)
        .bind(status.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete a recipient
    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM recipients WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all recipients in a list
    pub async fn delete_by_list(&self, recipient_list_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM recipients WHERE recipient_list_id = $1")
            .bind(recipient_list_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    /// Count recipients by list
    pub async fn count_by_list(
        &self,
        recipient_list_id: Uuid,
        status: Option<RecipientStatus>,
    ) -> Result<i64, sqlx::Error> {
        let count: (i64,) = if let Some(status) = status {
            sqlx::query_as(
                "SELECT COUNT(*) FROM recipients WHERE recipient_list_id = $1 AND status = $2",
            )
            .bind(recipient_list_id)
            .bind(status.to_string())
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM recipients WHERE recipient_list_id = $1")
                .bind(recipient_list_id)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(count.0)
    }

    /// Count active recipients by list
    pub async fn count_active_by_list(&self, recipient_list_id: Uuid) -> Result<i64, sqlx::Error> {
        self.count_by_list(recipient_list_id, Some(RecipientStatus::Active))
            .await
    }
}
