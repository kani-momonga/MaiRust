//! Scheduled Delivery Worker - Processes and sends scheduled messages

use super::manager::CampaignManager;
use super::rate_limiter::RateLimiter;
use anyhow::Result;
use chrono::{Duration, Utc};
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use mairust_storage::db::DatabasePool;
use mairust_storage::models::{ScheduledMessage, ScheduledMessageStatus};
use mairust_storage::repository::ScheduledMessageRepository;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Semaphore;
use tokio::time::{interval, Duration as TokioDuration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Result of a delivery attempt
#[derive(Debug)]
pub enum DeliveryResult {
    /// Successfully sent
    Sent { message_id: String },
    /// Temporarily failed, should retry
    TemporaryFailure { error: String },
    /// Permanently failed, should not retry
    PermanentFailure { error: String },
    /// Bounced
    Bounced { bounce_type: String, reason: String },
}

/// SMTP configuration for sending
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub use_tls: bool,
    pub use_starttls: bool,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 25,
            username: None,
            password: None,
            use_tls: false,
            use_starttls: true,
        }
    }
}

/// Scheduled Delivery Worker
pub struct ScheduledDeliveryWorker {
    db_pool: DatabasePool,
    scheduled_message_repo: ScheduledMessageRepository,
    campaign_manager: Arc<CampaignManager>,
    rate_limiter: Arc<RateLimiter>,
    smtp_config: SmtpConfig,
    /// Maximum concurrent sends
    concurrency_limit: usize,
    /// Batch size for fetching pending messages
    batch_size: i64,
    /// Interval between processing cycles (seconds)
    poll_interval_secs: u64,
}

impl ScheduledDeliveryWorker {
    /// Create a new scheduled delivery worker
    pub fn new(
        db_pool: DatabasePool,
        campaign_manager: Arc<CampaignManager>,
        smtp_config: SmtpConfig,
    ) -> Self {
        let pool = db_pool.pool().clone();
        let rate_limiter = campaign_manager.rate_limiter();

        Self {
            db_pool,
            scheduled_message_repo: ScheduledMessageRepository::new(pool),
            campaign_manager,
            rate_limiter,
            smtp_config,
            concurrency_limit: 10,
            batch_size: 100,
            poll_interval_secs: 5,
        }
    }

    /// Set concurrency limit
    pub fn with_concurrency_limit(mut self, limit: usize) -> Self {
        self.concurrency_limit = limit;
        self
    }

    /// Set batch size
    pub fn with_batch_size(mut self, size: i64) -> Self {
        self.batch_size = size;
        self
    }

    /// Set poll interval
    pub fn with_poll_interval(mut self, secs: u64) -> Self {
        self.poll_interval_secs = secs;
        self
    }

    /// Run the delivery worker
    pub async fn run(&self) {
        let mut ticker = interval(TokioDuration::from_secs(self.poll_interval_secs));
        let semaphore = Arc::new(Semaphore::new(self.concurrency_limit));

        info!(
            "Scheduled delivery worker started (concurrency: {}, batch: {}, interval: {}s)",
            self.concurrency_limit, self.batch_size, self.poll_interval_secs
        );

        loop {
            ticker.tick().await;

            if let Err(e) = self.process_pending_messages(&semaphore).await {
                error!("Error processing scheduled messages: {}", e);
            }

            // Check for campaign completions
            if let Err(e) = self.check_campaign_completions().await {
                error!("Error checking campaign completions: {}", e);
            }

            // Start scheduled campaigns
            if let Err(e) = self.start_scheduled_campaigns().await {
                error!("Error starting scheduled campaigns: {}", e);
            }

            // Periodic cleanup
            if let Err(e) = self.rate_limiter.cleanup_old_counters().await {
                warn!("Error cleaning up rate limit counters: {}", e);
            }
        }
    }

    /// Process pending messages
    async fn process_pending_messages(&self, semaphore: &Arc<Semaphore>) -> Result<()> {
        // Fetch pending messages that are due
        let messages = self
            .scheduled_message_repo
            .get_pending_ready(self.batch_size)
            .await?;

        if messages.is_empty() {
            return Ok(());
        }

        debug!("Processing {} pending messages", messages.len());

        let mut handles = Vec::new();

        for message in messages {
            // Check rate limit before processing
            if !self.rate_limiter.check_allowed(message.tenant_id).await? {
                debug!(
                    "Rate limit hit for tenant {}, skipping message {}",
                    message.tenant_id, message.id
                );
                continue;
            }

            // Mark as processing
            if !self
                .scheduled_message_repo
                .mark_processing(message.id)
                .await?
            {
                // Already picked up by another worker
                continue;
            }

            // Acquire semaphore permit
            let permit = semaphore.clone().acquire_owned().await?;
            let repo = self.scheduled_message_repo.clone();
            let smtp_config = self.smtp_config.clone();
            let rate_limiter = self.rate_limiter.clone();

            // Spawn send task
            let handle = tokio::spawn(async move {
                let result = Self::send_message(&smtp_config, &message).await;
                Self::handle_result(&repo, &rate_limiter, &message, result).await;
                drop(permit);
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Task error: {}", e);
            }
        }

        Ok(())
    }

    /// Send a single message via SMTP
    async fn send_message(smtp_config: &SmtpConfig, message: &ScheduledMessage) -> DeliveryResult {
        // Parse from address
        let from: Mailbox = match message.from_address.parse() {
            Ok(m) => m,
            Err(e) => {
                return DeliveryResult::PermanentFailure {
                    error: format!("Invalid from address: {}", e),
                };
            }
        };

        // Parse to address
        let to: Mailbox = match message.to_address.parse() {
            Ok(m) => m,
            Err(e) => {
                return DeliveryResult::PermanentFailure {
                    error: format!("Invalid to address: {}", e),
                };
            }
        };

        // Build message
        let mut email_builder = Message::builder()
            .from(from)
            .to(to)
            .subject(&message.subject);

        // Add custom headers
        if let Some(headers) = message.headers.as_object() {
            for (key, value) in headers {
                if let Some(v) = value.as_str() {
                    // Note: lettre has limited header support, so we'd need custom handling
                    // For now, we'll skip custom headers
                    debug!("Header {}: {}", key, v);
                }
            }
        }

        // Generate Message-ID
        let msg_id = format!("<{}.{}@mairust>", Uuid::new_v4(), Utc::now().timestamp());

        // Build body
        let email = match (&message.html_body, &message.text_body) {
            (Some(html), Some(text)) => {
                // Multipart alternative
                email_builder.multipart(
                    MultiPart::alternative()
                        .singlepart(SinglePart::plain(text.clone()))
                        .singlepart(SinglePart::html(html.clone())),
                )
            }
            (Some(html), None) => email_builder.header(ContentType::TEXT_HTML).body(html.clone()),
            (None, Some(text)) => {
                email_builder
                    .header(ContentType::TEXT_PLAIN)
                    .body(text.clone())
            }
            (None, None) => email_builder.body(String::new()),
        };

        let email = match email {
            Ok(e) => e,
            Err(e) => {
                return DeliveryResult::PermanentFailure {
                    error: format!("Failed to build email: {}", e),
                };
            }
        };

        // Build SMTP transport
        let transport_result = if smtp_config.use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_config.host)
        } else if smtp_config.use_starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_config.host)
        } else {
            return Self::send_with_transport_dangerous(smtp_config, email, &msg_id).await;
        };

        let mut transport = match transport_result {
            Ok(t) => t.port(smtp_config.port),
            Err(e) => {
                return DeliveryResult::TemporaryFailure {
                    error: format!("Failed to create SMTP transport: {}", e),
                };
            }
        };

        // Add credentials if configured
        if let (Some(username), Some(password)) = (&smtp_config.username, &smtp_config.password) {
            transport = transport.credentials(Credentials::new(username.clone(), password.clone()));
        }

        let mailer = transport.timeout(Some(StdDuration::from_secs(30))).build();

        // Send email
        match mailer.send(email).await {
            Ok(response) => {
                debug!("Email sent: {:?}", response);
                DeliveryResult::Sent { message_id: msg_id }
            }
            Err(e) => {
                let error_str = e.to_string();

                // Check for permanent vs temporary failure
                if error_str.contains("5.1.1")
                    || error_str.contains("550")
                    || error_str.contains("User unknown")
                    || error_str.contains("does not exist")
                {
                    DeliveryResult::Bounced {
                        bounce_type: "hard".to_string(),
                        reason: error_str,
                    }
                } else if error_str.contains("4")
                    || error_str.contains("temporarily")
                    || error_str.contains("try again")
                {
                    DeliveryResult::TemporaryFailure { error: error_str }
                } else {
                    DeliveryResult::PermanentFailure { error: error_str }
                }
            }
        }
    }

    /// Send with dangerous (unencrypted) transport
    async fn send_with_transport_dangerous(
        smtp_config: &SmtpConfig,
        email: Message,
        msg_id: &str,
    ) -> DeliveryResult {
        let mut transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp_config.host)
            .port(smtp_config.port);

        if let (Some(username), Some(password)) = (&smtp_config.username, &smtp_config.password) {
            transport = transport.credentials(Credentials::new(username.clone(), password.clone()));
        }

        let mailer = transport.timeout(Some(StdDuration::from_secs(30))).build();

        match mailer.send(email).await {
            Ok(_) => DeliveryResult::Sent {
                message_id: msg_id.to_string(),
            },
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("5.1.1")
                    || error_str.contains("550")
                    || error_str.contains("User unknown")
                {
                    DeliveryResult::Bounced {
                        bounce_type: "hard".to_string(),
                        reason: error_str,
                    }
                } else {
                    DeliveryResult::TemporaryFailure { error: error_str }
                }
            }
        }
    }

    /// Handle the result of a delivery attempt
    async fn handle_result(
        repo: &ScheduledMessageRepository,
        rate_limiter: &RateLimiter,
        message: &ScheduledMessage,
        result: DeliveryResult,
    ) {
        match result {
            DeliveryResult::Sent { message_id } => {
                info!(
                    "Message {} sent successfully (Message-ID: {})",
                    message.id, message_id
                );

                if let Err(e) = repo.mark_sent(message.id, &message_id).await {
                    error!("Failed to mark message {} as sent: {}", message.id, e);
                }

                // Increment rate limit counter
                if let Err(e) = rate_limiter.increment(message.tenant_id).await {
                    warn!("Failed to increment rate limit counter: {}", e);
                }
            }

            DeliveryResult::TemporaryFailure { error } => {
                warn!("Message {} temporary failure: {}", message.id, error);

                if let Err(e) = repo.mark_failed(message.id, &error).await {
                    error!("Failed to mark message {} as failed: {}", message.id, e);
                }
            }

            DeliveryResult::PermanentFailure { error } => {
                error!("Message {} permanent failure: {}", message.id, error);

                // Mark as failed without retry
                let pool = repo.pool();
                if let Err(e) = sqlx::query(
                    "UPDATE scheduled_messages SET status = 'failed', last_error = $2, attempts = max_attempts WHERE id = $1"
                )
                .bind(message.id)
                .bind(&error)
                .execute(pool)
                .await {
                    error!("Failed to mark message {} as permanently failed: {}", message.id, e);
                }
            }

            DeliveryResult::Bounced { bounce_type, reason } => {
                warn!(
                    "Message {} bounced ({}): {}",
                    message.id, bounce_type, reason
                );

                if let Err(e) = repo.mark_bounced(message.id, &bounce_type, &reason).await {
                    error!("Failed to mark message {} as bounced: {}", message.id, e);
                }
            }
        }
    }

    /// Check for completed campaigns
    async fn check_campaign_completions(&self) -> Result<()> {
        // Get all sending campaigns
        let pool = self.db_pool.pool();
        let campaigns: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM campaigns WHERE status = 'sending'"
        )
        .fetch_all(pool)
        .await?;

        for (campaign_id,) in campaigns {
            self.campaign_manager
                .check_campaign_completion(campaign_id)
                .await?;
        }

        Ok(())
    }

    /// Start scheduled campaigns that are ready
    async fn start_scheduled_campaigns(&self) -> Result<()> {
        let campaigns = self.campaign_manager.get_scheduled_ready().await?;

        for campaign in campaigns {
            info!(
                "Starting scheduled campaign {} (scheduled_at: {:?})",
                campaign.id, campaign.scheduled_at
            );

            // Update to sending status
            let pool = self.db_pool.pool();
            sqlx::query(
                "UPDATE campaigns SET status = 'sending', started_at = NOW(), updated_at = NOW() WHERE id = $1"
            )
            .bind(campaign.id)
            .execute(pool)
            .await?;
        }

        Ok(())
    }
}

