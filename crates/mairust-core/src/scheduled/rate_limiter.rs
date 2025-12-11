//! Rate Limiter - Controls sending rate per tenant

use anyhow::Result;
use chrono::{Duration, Timelike, Utc};
use mairust_common::types::TenantId;
use mairust_storage::db::DatabasePool;
use mairust_storage::models::TenantRateLimit;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Rate limiter for controlling send rates per tenant
pub struct RateLimiter {
    db_pool: DatabasePool,
    /// Cache of rate limits per tenant
    cache: Arc<RwLock<std::collections::HashMap<TenantId, TenantRateLimit>>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(db_pool: DatabasePool) -> Self {
        Self {
            db_pool,
            cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Check if sending is allowed for the tenant
    pub async fn check_allowed(&self, tenant_id: TenantId) -> Result<bool> {
        // Get rate limits for tenant
        let limits = self.get_rate_limits(tenant_id).await?;

        if !limits.enabled {
            return Ok(true);
        }

        let pool = self.db_pool.pool();

        // Check minute window
        let minute_count = self.get_window_count(tenant_id, "minute").await?;
        if minute_count >= limits.per_minute as i64 {
            debug!(
                "Rate limit hit for tenant {} (minute): {} >= {}",
                tenant_id, minute_count, limits.per_minute
            );
            return Ok(false);
        }

        // Check hour window
        let hour_count = self.get_window_count(tenant_id, "hour").await?;
        if hour_count >= limits.per_hour as i64 {
            debug!(
                "Rate limit hit for tenant {} (hour): {} >= {}",
                tenant_id, hour_count, limits.per_hour
            );
            return Ok(false);
        }

        // Check day window
        let day_count = self.get_window_count(tenant_id, "day").await?;
        if day_count >= limits.per_day as i64 {
            debug!(
                "Rate limit hit for tenant {} (day): {} >= {}",
                tenant_id, day_count, limits.per_day
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Increment the counter after sending
    pub async fn increment(&self, tenant_id: TenantId) -> Result<()> {
        let limits = self.get_rate_limits(tenant_id).await?;

        if !limits.enabled {
            return Ok(());
        }

        let pool = self.db_pool.pool();
        let now = Utc::now();

        // Update minute window
        let minute_start = now
            .with_second(0)
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(now);

        self.upsert_counter(tenant_id, "minute", minute_start, limits.per_minute)
            .await?;

        // Update hour window
        let hour_start = minute_start.with_minute(0).unwrap_or(minute_start);

        self.upsert_counter(tenant_id, "hour", hour_start, limits.per_hour)
            .await?;

        // Update day window
        let day_start = hour_start.with_hour(0).unwrap_or(hour_start);

        self.upsert_counter(tenant_id, "day", day_start, limits.per_day)
            .await?;

        Ok(())
    }

    /// Get rate limits for a tenant (with caching)
    async fn get_rate_limits(&self, tenant_id: TenantId) -> Result<TenantRateLimit> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(limits) = cache.get(&tenant_id) {
                return Ok(limits.clone());
            }
        }

        // Fetch from database
        let pool = self.db_pool.pool();
        let limits: Option<TenantRateLimit> = sqlx::query_as(
            r#"SELECT * FROM tenant_rate_limits WHERE tenant_id = $1"#,
        )
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?;

        let limits = limits.unwrap_or_else(|| {
            let mut default = TenantRateLimit::default();
            default.tenant_id = tenant_id;
            default
        });

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(tenant_id, limits.clone());
        }

        Ok(limits)
    }

    /// Get current count for a window
    async fn get_window_count(&self, tenant_id: TenantId, window_type: &str) -> Result<i64> {
        let pool = self.db_pool.pool();
        let now = Utc::now();

        let window_start = match window_type {
            "minute" => now
                .with_second(0)
                .and_then(|t| t.with_nanosecond(0))
                .unwrap_or(now),
            "hour" => now
                .with_second(0)
                .and_then(|t| t.with_nanosecond(0))
                .and_then(|t| t.with_minute(0))
                .unwrap_or(now),
            "day" => now
                .with_second(0)
                .and_then(|t| t.with_nanosecond(0))
                .and_then(|t| t.with_minute(0))
                .and_then(|t| t.with_hour(0))
                .unwrap_or(now),
            _ => return Ok(0),
        };

        let count: Option<(i32,)> = sqlx::query_as(
            r#"
            SELECT count FROM rate_limit_counters
            WHERE tenant_id = $1 AND window_type = $2 AND window_start = $3
            "#,
        )
        .bind(tenant_id)
        .bind(window_type)
        .bind(window_start)
        .fetch_optional(pool)
        .await?;

        Ok(count.map(|(c,)| c as i64).unwrap_or(0))
    }

    /// Upsert counter for a window
    async fn upsert_counter(
        &self,
        tenant_id: TenantId,
        window_type: &str,
        window_start: chrono::DateTime<Utc>,
        limit_value: i32,
    ) -> Result<()> {
        let pool = self.db_pool.pool();

        sqlx::query(
            r#"
            INSERT INTO rate_limit_counters (id, tenant_id, window_type, window_start, count, limit_value)
            VALUES (gen_random_uuid(), $1, $2, $3, 1, $4)
            ON CONFLICT (tenant_id, window_type, window_start)
            DO UPDATE SET count = rate_limit_counters.count + 1, updated_at = NOW()
            "#,
        )
        .bind(tenant_id)
        .bind(window_type)
        .bind(window_start)
        .bind(limit_value)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get remaining quota for a tenant
    pub async fn get_remaining(&self, tenant_id: TenantId) -> Result<RemainingQuota> {
        let limits = self.get_rate_limits(tenant_id).await?;

        if !limits.enabled {
            return Ok(RemainingQuota {
                per_minute: i32::MAX,
                per_hour: i32::MAX,
                per_day: i32::MAX,
            });
        }

        let minute_count = self.get_window_count(tenant_id, "minute").await? as i32;
        let hour_count = self.get_window_count(tenant_id, "hour").await? as i32;
        let day_count = self.get_window_count(tenant_id, "day").await? as i32;

        Ok(RemainingQuota {
            per_minute: (limits.per_minute - minute_count).max(0),
            per_hour: (limits.per_hour - hour_count).max(0),
            per_day: (limits.per_day - day_count).max(0),
        })
    }

    /// Clear rate limit cache for a tenant
    pub async fn clear_cache(&self, tenant_id: TenantId) {
        let mut cache = self.cache.write().await;
        cache.remove(&tenant_id);
    }

    /// Cleanup old counters (should be run periodically)
    pub async fn cleanup_old_counters(&self) -> Result<u64> {
        let pool = self.db_pool.pool();

        // Delete counters older than 2 days
        let cutoff = Utc::now() - Duration::days(2);

        let result = sqlx::query(
            r#"DELETE FROM rate_limit_counters WHERE window_start < $1"#,
        )
        .bind(cutoff)
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }
}

/// Remaining quota for a tenant
#[derive(Debug, Clone)]
pub struct RemainingQuota {
    pub per_minute: i32,
    pub per_hour: i32,
    pub per_day: i32,
}

impl RemainingQuota {
    /// Get the minimum remaining across all windows
    pub fn min(&self) -> i32 {
        self.per_minute.min(self.per_hour).min(self.per_day)
    }
}
