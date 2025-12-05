//! Domain settings repository

use crate::db::DatabasePool;
use crate::models::{DomainSettings, UpdateDomainSettings};
use async_trait::async_trait;
use mairust_common::types::DomainId;
use mairust_common::{Error, Result};

/// Domain settings repository trait
#[async_trait]
pub trait DomainSettingsRepository: Send + Sync {
    async fn get(&self, domain_id: DomainId) -> Result<Option<DomainSettings>>;
    async fn get_or_create(&self, domain_id: DomainId) -> Result<DomainSettings>;
    async fn update(&self, domain_id: DomainId, input: UpdateDomainSettings) -> Result<DomainSettings>;
    async fn delete(&self, domain_id: DomainId) -> Result<()>;
}

/// Database domain settings repository
pub struct DbDomainSettingsRepository {
    pool: DatabasePool,
}

impl DbDomainSettingsRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DomainSettingsRepository for DbDomainSettingsRepository {
    async fn get(&self, domain_id: DomainId) -> Result<Option<DomainSettings>> {
        sqlx::query_as::<_, DomainSettings>("SELECT * FROM domain_settings WHERE domain_id = $1")
            .bind(domain_id)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn get_or_create(&self, domain_id: DomainId) -> Result<DomainSettings> {
        if let Some(settings) = self.get(domain_id).await? {
            return Ok(settings);
        }

        let now = chrono::Utc::now();
        let default_settings = DomainSettings::default();

        sqlx::query(
            r#"
            INSERT INTO domain_settings (
                domain_id, catch_all_enabled, catch_all_mailbox_id,
                max_message_size, max_recipients, rate_limit_per_hour,
                require_tls_inbound, require_tls_outbound,
                spf_policy, dmarc_policy, extra_settings, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(domain_id)
        .bind(default_settings.catch_all_enabled)
        .bind(default_settings.catch_all_mailbox_id)
        .bind(default_settings.max_message_size)
        .bind(default_settings.max_recipients)
        .bind(default_settings.rate_limit_per_hour)
        .bind(default_settings.require_tls_inbound)
        .bind(default_settings.require_tls_outbound)
        .bind(&default_settings.spf_policy)
        .bind(&default_settings.dmarc_policy)
        .bind(&default_settings.extra_settings)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(domain_id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create domain settings".to_string()))
    }

    async fn update(&self, domain_id: DomainId, input: UpdateDomainSettings) -> Result<DomainSettings> {
        // Ensure settings exist
        let _current = self.get_or_create(domain_id).await?;
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            UPDATE domain_settings SET
                catch_all_enabled = COALESCE($2, catch_all_enabled),
                catch_all_mailbox_id = COALESCE($3, catch_all_mailbox_id),
                max_message_size = COALESCE($4, max_message_size),
                max_recipients = COALESCE($5, max_recipients),
                rate_limit_per_hour = COALESCE($6, rate_limit_per_hour),
                require_tls_inbound = COALESCE($7, require_tls_inbound),
                require_tls_outbound = COALESCE($8, require_tls_outbound),
                spf_policy = COALESCE($9, spf_policy),
                dmarc_policy = COALESCE($10, dmarc_policy),
                extra_settings = COALESCE($11, extra_settings),
                updated_at = $12
            WHERE domain_id = $1
            "#,
        )
        .bind(domain_id)
        .bind(input.catch_all_enabled)
        .bind(input.catch_all_mailbox_id)
        .bind(input.max_message_size)
        .bind(input.max_recipients)
        .bind(input.rate_limit_per_hour)
        .bind(input.require_tls_inbound)
        .bind(input.require_tls_outbound)
        .bind(input.spf_policy)
        .bind(input.dmarc_policy)
        .bind(input.extra_settings)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(domain_id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to update domain settings".to_string()))
    }

    async fn delete(&self, domain_id: DomainId) -> Result<()> {
        sqlx::query("DELETE FROM domain_settings WHERE domain_id = $1")
            .bind(domain_id)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
