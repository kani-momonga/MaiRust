//! Queue Manager - Handles outbound mail queue and delivery

use crate::hooks::HookManager;
use anyhow::Result;
use chrono::{Duration, Utc};
use mairust_storage::db::DatabasePool;
use mairust_storage::file::FileStorage;
use mairust_storage::models::Job;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{interval, Duration as TokioDuration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Job payload for outbound mail delivery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryJob {
    pub message_id: Uuid,
    pub tenant_id: Uuid,
    pub from: String,
    pub to: Vec<String>,
    pub storage_path: String,
}

/// Queue Manager for handling mail delivery
pub struct QueueManager<S: FileStorage> {
    db_pool: DatabasePool,
    file_storage: Arc<S>,
    hook_manager: Arc<HookManager>,
}

impl<S: FileStorage + Send + Sync + 'static> QueueManager<S> {
    /// Create a new queue manager
    pub fn new(
        db_pool: DatabasePool,
        file_storage: Arc<S>,
        hook_manager: Arc<HookManager>,
    ) -> Self {
        Self {
            db_pool,
            file_storage,
            hook_manager,
        }
    }

    /// Run the queue processor
    pub async fn run(&self) {
        let mut ticker = interval(TokioDuration::from_secs(5));

        info!("Queue processor started");

        loop {
            ticker.tick().await;

            if let Err(e) = self.process_pending_jobs().await {
                error!("Error processing queue: {}", e);
            }
        }
    }

    /// Enqueue a delivery job
    pub async fn enqueue_delivery(&self, job: DeliveryJob) -> Result<Uuid> {
        let job_id = Uuid::now_v7();

        let db_job = Job {
            id: job_id,
            queue: "delivery".to_string(),
            payload: serde_json::to_value(&job)?,
            status: "pending".to_string(),
            attempts: 0,
            max_attempts: 5,
            last_error: None,
            scheduled_at: Utc::now(),
            started_at: None,
            completed_at: None,
            created_at: Utc::now(),
        };

        let pool = self.db_pool.pool();
        sqlx::query(
            r#"
            INSERT INTO jobs (id, queue, payload, status, attempts, max_attempts, scheduled_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(db_job.id)
        .bind(&db_job.queue)
        .bind(&db_job.payload)
        .bind(&db_job.status)
        .bind(db_job.attempts)
        .bind(db_job.max_attempts)
        .bind(db_job.scheduled_at)
        .bind(db_job.created_at)
        .execute(pool)
        .await?;

        info!("Enqueued delivery job {}", job_id);
        Ok(job_id)
    }

    /// Process pending jobs
    async fn process_pending_jobs(&self) -> Result<()> {
        let pool = self.db_pool.pool();

        // Fetch pending jobs that are due
        let jobs: Vec<Job> = sqlx::query_as(
            r#"
            SELECT * FROM jobs
            WHERE status = 'pending'
            AND queue = 'delivery'
            AND scheduled_at <= NOW()
            ORDER BY scheduled_at ASC
            LIMIT 10
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .fetch_all(pool)
        .await?;

        for job in jobs {
            self.process_job(job).await;
        }

        Ok(())
    }

    /// Process a single job
    async fn process_job(&self, job: Job) {
        let job_id = job.id;
        debug!("Processing job {}", job_id);

        // Mark job as in progress
        if let Err(e) = self.mark_job_started(job_id).await {
            error!("Failed to mark job {} as started: {}", job_id, e);
            return;
        }

        // Parse job payload
        let delivery_job: DeliveryJob = match serde_json::from_value(job.payload) {
            Ok(j) => j,
            Err(e) => {
                error!("Failed to parse job {} payload: {}", job_id, e);
                let _ = self.mark_job_failed(job_id, &e.to_string()).await;
                return;
            }
        };

        // Execute delivery
        match self.deliver_message(&delivery_job).await {
            Ok(()) => {
                info!("Job {} completed successfully", job_id);
                if let Err(e) = self.mark_job_completed(job_id).await {
                    error!("Failed to mark job {} as completed: {}", job_id, e);
                }
            }
            Err(e) => {
                warn!("Job {} failed: {}", job_id, e);

                let attempts = job.attempts + 1;
                if attempts >= job.max_attempts {
                    error!("Job {} exceeded max attempts, marking as failed", job_id);
                    let _ = self.mark_job_failed(job_id, &e.to_string()).await;
                } else {
                    // Schedule retry with exponential backoff
                    let delay = calculate_backoff(attempts);
                    let _ = self.schedule_retry(job_id, attempts, &e.to_string(), delay).await;
                }
            }
        }
    }

    /// Deliver a message
    async fn deliver_message(&self, job: &DeliveryJob) -> Result<()> {
        // Read message from storage
        let _data = self.file_storage.retrieve(&job.storage_path).await?;

        // Execute pre_send hooks
        // Note: In production, we'd load the full message and execute hooks

        // Group recipients by domain
        let mut by_domain: std::collections::HashMap<String, Vec<&str>> =
            std::collections::HashMap::new();

        for recipient in &job.to {
            if let Some(domain) = recipient.split('@').nth(1) {
                by_domain
                    .entry(domain.to_string())
                    .or_default()
                    .push(recipient);
            }
        }

        // Deliver to each domain
        for (domain, recipients) in by_domain {
            self.deliver_to_domain(&domain, &recipients, &job.from, job)
                .await?;
        }

        Ok(())
    }

    /// Deliver message to a specific domain
    async fn deliver_to_domain(
        &self,
        domain: &str,
        _recipients: &[&str],
        _from: &str,
        _job: &DeliveryJob,
    ) -> Result<()> {
        // Resolve MX records
        let _mx_hosts = self.resolve_mx(domain).await?;

        // For Phase 1, we'll just log the delivery attempt
        // In production, we'd connect to the MX and deliver via SMTP
        info!("Would deliver to domain {} (MX resolution done)", domain);

        // TODO: Implement actual SMTP delivery
        // 1. Connect to MX host
        // 2. Send EHLO
        // 3. STARTTLS if supported
        // 4. MAIL FROM / RCPT TO / DATA
        // 5. Handle response codes

        Ok(())
    }

    /// Resolve MX records for a domain
    async fn resolve_mx(&self, domain: &str) -> Result<Vec<String>> {
        use trust_dns_resolver::config::*;
        use trust_dns_resolver::TokioAsyncResolver;

        let resolver = TokioAsyncResolver::tokio(
            ResolverConfig::default(),
            ResolverOpts::default(),
        );

        let mx_response = resolver.mx_lookup(domain).await;

        match mx_response {
            Ok(mx) => {
                let mut hosts: Vec<(u16, String)> = mx
                    .iter()
                    .map(|r| (r.preference(), r.exchange().to_string()))
                    .collect();

                // Sort by preference (lower is better)
                hosts.sort_by_key(|(pref, _)| *pref);

                Ok(hosts.into_iter().map(|(_, host)| host).collect())
            }
            Err(e) => {
                // If no MX, try A record
                warn!("No MX records for {}, falling back to A record: {}", domain, e);
                Ok(vec![domain.to_string()])
            }
        }
    }

    /// Mark a job as started
    async fn mark_job_started(&self, job_id: Uuid) -> Result<()> {
        let pool = self.db_pool.pool();
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'processing', started_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Mark a job as completed
    async fn mark_job_completed(&self, job_id: Uuid) -> Result<()> {
        let pool = self.db_pool.pool();
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'completed', completed_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Mark a job as failed
    async fn mark_job_failed(&self, job_id: Uuid, error: &str) -> Result<()> {
        let pool = self.db_pool.pool();
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'failed', last_error = $2, completed_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(error)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Schedule a job retry
    async fn schedule_retry(
        &self,
        job_id: Uuid,
        attempts: i32,
        error: &str,
        delay: Duration,
    ) -> Result<()> {
        let pool = self.db_pool.pool();
        let scheduled_at = Utc::now() + delay;

        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'pending',
                attempts = $2,
                last_error = $3,
                scheduled_at = $4
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(attempts)
        .bind(error)
        .bind(scheduled_at)
        .execute(pool)
        .await?;

        info!(
            "Job {} scheduled for retry at {} (attempt {})",
            job_id, scheduled_at, attempts + 1
        );

        Ok(())
    }

    /// Get queue statistics
    pub async fn get_stats(&self) -> Result<QueueStats> {
        let pool = self.db_pool.pool();

        let pending: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM jobs WHERE status = 'pending' AND queue = 'delivery'",
        )
        .fetch_one(pool)
        .await?;

        let processing: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM jobs WHERE status = 'processing' AND queue = 'delivery'",
        )
        .fetch_one(pool)
        .await?;

        let failed: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM jobs WHERE status = 'failed' AND queue = 'delivery'",
        )
        .fetch_one(pool)
        .await?;

        Ok(QueueStats {
            pending: pending.0 as u64,
            processing: processing.0 as u64,
            failed: failed.0 as u64,
        })
    }
}

/// Calculate exponential backoff delay
fn calculate_backoff(attempts: i32) -> Duration {
    // Base: 1 minute, max: 4 hours
    let minutes = std::cmp::min(2_i64.pow(attempts as u32), 240);
    Duration::minutes(minutes)
}

/// Queue statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub pending: u64,
    pub processing: u64,
    pub failed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_backoff() {
        assert_eq!(calculate_backoff(0), Duration::minutes(1));
        assert_eq!(calculate_backoff(1), Duration::minutes(2));
        assert_eq!(calculate_backoff(2), Duration::minutes(4));
        assert_eq!(calculate_backoff(3), Duration::minutes(8));
        assert_eq!(calculate_backoff(10), Duration::minutes(240)); // Max capped at 4 hours
    }
}
