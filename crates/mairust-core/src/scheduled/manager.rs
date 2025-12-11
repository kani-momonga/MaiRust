//! Campaign Manager - Handles campaign lifecycle and message scheduling

use super::rate_limiter::RateLimiter;
use super::template::TemplateRenderer;
use anyhow::Result;
use chrono::{Duration, Utc};
use mairust_common::types::TenantId;
use mairust_storage::db::DatabasePool;
use mairust_storage::models::{
    Campaign, CampaignStats, CampaignStatus, CreateScheduledMessage, Recipient,
    RecipientStatus, ScheduledMessageStatus,
};
use mairust_storage::repository::{
    CampaignMessageCounts, CampaignRepository, RecipientListRepository, RecipientRepository,
    ScheduledMessageRepository, UnsubscribeRepository,
};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Campaign manager errors
#[derive(Error, Debug)]
pub enum CampaignError {
    #[error("Campaign not found")]
    NotFound,

    #[error("Campaign is not in draft status")]
    NotDraft,

    #[error("Campaign is not in scheduled or sending status")]
    NotScheduledOrSending,

    #[error("Campaign has no recipient list")]
    NoRecipientList,

    #[error("Recipient list is empty")]
    EmptyRecipientList,

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

/// Campaign Manager - Manages campaign lifecycle
pub struct CampaignManager {
    db_pool: DatabasePool,
    campaign_repo: CampaignRepository,
    recipient_list_repo: RecipientListRepository,
    recipient_repo: RecipientRepository,
    scheduled_message_repo: ScheduledMessageRepository,
    unsubscribe_repo: UnsubscribeRepository,
    rate_limiter: Arc<RateLimiter>,
    template_renderer: TemplateRenderer,
}

impl CampaignManager {
    /// Create a new campaign manager
    pub fn new(db_pool: DatabasePool, unsubscribe_base_url: String) -> Self {
        let pool = db_pool.pool().clone();
        Self {
            db_pool: db_pool.clone(),
            campaign_repo: CampaignRepository::new(pool.clone()),
            recipient_list_repo: RecipientListRepository::new(pool.clone()),
            recipient_repo: RecipientRepository::new(pool.clone()),
            scheduled_message_repo: ScheduledMessageRepository::new(pool.clone()),
            unsubscribe_repo: UnsubscribeRepository::new(pool.clone()),
            rate_limiter: Arc::new(RateLimiter::new(db_pool)),
            template_renderer: TemplateRenderer::new(unsubscribe_base_url),
        }
    }

    /// Get the rate limiter
    pub fn rate_limiter(&self) -> Arc<RateLimiter> {
        Arc::clone(&self.rate_limiter)
    }

    /// Schedule a campaign for sending
    pub async fn schedule_campaign(
        &self,
        tenant_id: TenantId,
        campaign_id: Uuid,
        scheduled_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<Campaign, CampaignError> {
        // Get campaign
        let campaign = self
            .campaign_repo
            .get_by_tenant(tenant_id, campaign_id)
            .await?
            .ok_or(CampaignError::NotFound)?;

        // Validate status
        if campaign.status != "draft" {
            return Err(CampaignError::NotDraft);
        }

        // Validate recipient list
        let recipient_list_id = campaign
            .recipient_list_id
            .ok_or(CampaignError::NoRecipientList)?;

        // Get active recipient count
        let active_count = self
            .recipient_repo
            .count_active_by_list(recipient_list_id)
            .await?;

        if active_count == 0 {
            return Err(CampaignError::EmptyRecipientList);
        }

        // Update total recipients
        self.campaign_repo
            .set_total_recipients(campaign_id, active_count as i32)
            .await?;

        // Create scheduled messages
        let start_time = scheduled_at.unwrap_or_else(Utc::now);
        self.create_scheduled_messages(&campaign, start_time)
            .await?;

        // Update campaign status
        let updated = self
            .campaign_repo
            .update_status(campaign_id, CampaignStatus::Scheduled)
            .await?
            .ok_or(CampaignError::NotFound)?;

        info!(
            "Campaign {} scheduled with {} recipients, starting at {}",
            campaign_id, active_count, start_time
        );

        Ok(updated)
    }

    /// Start sending a scheduled campaign immediately
    pub async fn start_campaign(
        &self,
        tenant_id: TenantId,
        campaign_id: Uuid,
    ) -> Result<Campaign, CampaignError> {
        let campaign = self
            .campaign_repo
            .get_by_tenant(tenant_id, campaign_id)
            .await?
            .ok_or(CampaignError::NotFound)?;

        if campaign.status != "scheduled" && campaign.status != "draft" {
            return Err(CampaignError::NotDraft);
        }

        // If draft, schedule first
        if campaign.status == "draft" {
            self.schedule_campaign(tenant_id, campaign_id, Some(Utc::now()))
                .await?;
        }

        // Update to sending status
        let updated = self
            .campaign_repo
            .update_status(campaign_id, CampaignStatus::Sending)
            .await?
            .ok_or(CampaignError::NotFound)?;

        info!("Campaign {} started sending", campaign_id);

        Ok(updated)
    }

    /// Pause a sending campaign
    pub async fn pause_campaign(
        &self,
        tenant_id: TenantId,
        campaign_id: Uuid,
    ) -> Result<Campaign, CampaignError> {
        let campaign = self
            .campaign_repo
            .get_by_tenant(tenant_id, campaign_id)
            .await?
            .ok_or(CampaignError::NotFound)?;

        if campaign.status != "sending" {
            return Err(CampaignError::NotScheduledOrSending);
        }

        let updated = self
            .campaign_repo
            .update_status(campaign_id, CampaignStatus::Paused)
            .await?
            .ok_or(CampaignError::NotFound)?;

        info!("Campaign {} paused", campaign_id);

        Ok(updated)
    }

    /// Resume a paused campaign
    pub async fn resume_campaign(
        &self,
        tenant_id: TenantId,
        campaign_id: Uuid,
    ) -> Result<Campaign, CampaignError> {
        let campaign = self
            .campaign_repo
            .get_by_tenant(tenant_id, campaign_id)
            .await?
            .ok_or(CampaignError::NotFound)?;

        if campaign.status != "paused" {
            return Err(CampaignError::NotScheduledOrSending);
        }

        let updated = self
            .campaign_repo
            .update_status(campaign_id, CampaignStatus::Sending)
            .await?
            .ok_or(CampaignError::NotFound)?;

        info!("Campaign {} resumed", campaign_id);

        Ok(updated)
    }

    /// Cancel a campaign
    pub async fn cancel_campaign(
        &self,
        tenant_id: TenantId,
        campaign_id: Uuid,
    ) -> Result<Campaign, CampaignError> {
        let campaign = self
            .campaign_repo
            .get_by_tenant(tenant_id, campaign_id)
            .await?
            .ok_or(CampaignError::NotFound)?;

        if campaign.status != "scheduled"
            && campaign.status != "sending"
            && campaign.status != "paused"
        {
            return Err(CampaignError::NotScheduledOrSending);
        }

        // Cancel all pending messages
        let cancelled = self
            .scheduled_message_repo
            .cancel_by_campaign(campaign_id)
            .await?;

        // Update campaign status
        let updated = self
            .campaign_repo
            .update_status(campaign_id, CampaignStatus::Cancelled)
            .await?
            .ok_or(CampaignError::NotFound)?;

        info!(
            "Campaign {} cancelled, {} pending messages cancelled",
            campaign_id, cancelled
        );

        Ok(updated)
    }

    /// Get campaign statistics
    pub async fn get_campaign_stats(
        &self,
        tenant_id: TenantId,
        campaign_id: Uuid,
    ) -> Result<CampaignStats, CampaignError> {
        let campaign = self
            .campaign_repo
            .get_by_tenant(tenant_id, campaign_id)
            .await?
            .ok_or(CampaignError::NotFound)?;

        let counts = self
            .scheduled_message_repo
            .get_campaign_status_counts(campaign_id)
            .await?;

        let unsubscribed = self
            .unsubscribe_repo
            .count_by_campaign(campaign_id)
            .await?;

        // Calculate estimated completion
        let estimated_completion = if campaign.status == "sending" && counts.pending > 0 {
            let remaining = counts.pending;
            let rate = campaign.rate_limit_per_hour as i64;
            let hours_remaining = (remaining as f64 / rate as f64).ceil() as i64;
            Some(Utc::now() + Duration::hours(hours_remaining))
        } else {
            None
        };

        // Calculate current sending rate
        let current_rate = if campaign.status == "sending" {
            // In a real implementation, calculate actual rate from recent sends
            campaign.rate_limit_per_hour.min(campaign.rate_limit_per_minute * 60)
        } else {
            0
        };

        let progress = campaign.progress_percentage();
        Ok(CampaignStats {
            campaign_id,
            status: campaign.status,
            total_recipients: campaign.total_recipients,
            sent: (counts.sent + counts.delivered) as i32,
            delivered: counts.delivered as i32,
            bounced: counts.bounced as i32,
            failed: counts.failed as i32,
            opened: campaign.opened_count,
            clicked: campaign.clicked_count,
            unsubscribed: unsubscribed as i32,
            progress_percentage: progress,
            estimated_completion,
            current_rate,
            rate_limit_per_hour: campaign.rate_limit_per_hour,
        })
    }

    /// Create scheduled messages for a campaign
    async fn create_scheduled_messages(
        &self,
        campaign: &Campaign,
        start_time: chrono::DateTime<Utc>,
    ) -> Result<(), CampaignError> {
        let recipient_list_id = campaign
            .recipient_list_id
            .ok_or(CampaignError::NoRecipientList)?;

        let batch_size = 1000i64;
        let rate_per_minute = campaign.rate_limit_per_minute as usize;
        let batch_id = Uuid::new_v4();

        let mut offset = 0i64;
        let mut current_time = start_time;
        let mut minute_count = 0usize;

        loop {
            // Fetch batch of active recipients
            let recipients = self
                .recipient_repo
                .list_active_by_list(recipient_list_id, batch_size, offset)
                .await?;

            if recipients.is_empty() {
                break;
            }

            // Filter out unsubscribed emails
            let emails: Vec<String> = recipients.iter().map(|r| r.email.clone()).collect();
            let unsubscribed = self
                .unsubscribe_repo
                .filter_unsubscribed(campaign.tenant_id, &emails)
                .await?;

            let unsubscribed_set: std::collections::HashSet<_> = unsubscribed.into_iter().collect();

            // Create scheduled messages
            let mut messages = Vec::new();
            for recipient in recipients {
                // Skip unsubscribed
                if unsubscribed_set.contains(&recipient.email) {
                    continue;
                }

                // Render personalized content
                let subject = self.template_renderer.render_subject(
                    &campaign.subject,
                    &recipient,
                );

                let html_body = campaign.html_body.as_ref().map(|body| {
                    self.template_renderer.render(body, &recipient, Some(campaign.id))
                });

                let text_body = campaign.text_body.as_ref().map(|body| {
                    self.template_renderer.render(body, &recipient, Some(campaign.id))
                });

                // Generate headers
                let mut headers = serde_json::Map::new();

                // Add List-Unsubscribe header
                let list_unsubscribe = self.template_renderer.generate_list_unsubscribe_header(
                    &recipient.email,
                    Some(campaign.id),
                    None, // TODO: Add mailto address
                );
                headers.insert("List-Unsubscribe".to_string(), serde_json::json!(list_unsubscribe));
                headers.insert("List-Unsubscribe-Post".to_string(), serde_json::json!("List-Unsubscribe=One-Click"));

                // Add from name if specified
                let from_address = if let Some(ref name) = campaign.from_name {
                    format!("{} <{}>", name, campaign.from_address)
                } else {
                    campaign.from_address.clone()
                };

                messages.push(CreateScheduledMessage {
                    tenant_id: campaign.tenant_id,
                    campaign_id: Some(campaign.id),
                    recipient_id: Some(recipient.id),
                    batch_id: Some(batch_id),
                    from_address,
                    to_address: recipient.email.clone(),
                    subject,
                    html_body,
                    text_body,
                    headers: Some(serde_json::Value::Object(headers)),
                    scheduled_at: current_time,
                    max_attempts: Some(3),
                    metadata: Some(serde_json::json!({
                        "recipient_name": recipient.name,
                        "campaign_name": campaign.name,
                    })),
                });

                // Update timing for rate limiting
                minute_count += 1;
                if minute_count >= rate_per_minute {
                    current_time = current_time + Duration::minutes(1);
                    minute_count = 0;
                }
            }

            // Batch insert
            if !messages.is_empty() {
                self.scheduled_message_repo.create_batch(messages).await?;
            }

            offset += batch_size;
        }

        Ok(())
    }

    /// Check and update campaign status based on message states
    pub async fn check_campaign_completion(&self, campaign_id: Uuid) -> Result<bool> {
        let counts = self
            .scheduled_message_repo
            .get_campaign_status_counts(campaign_id)
            .await?;

        // Campaign is complete when no pending or processing messages
        if counts.pending == 0 && counts.processing == 0 {
            self.campaign_repo
                .update_status(campaign_id, CampaignStatus::Completed)
                .await?;

            info!("Campaign {} completed", campaign_id);
            return Ok(true);
        }

        Ok(false)
    }

    /// Get scheduled campaigns ready to start
    pub async fn get_scheduled_ready(&self) -> Result<Vec<Campaign>> {
        let campaigns = self.campaign_repo.get_scheduled_ready().await?;
        Ok(campaigns)
    }
}
