//! Hook Manager - Executes hooks and manages plugin calls

use anyhow::Result;
use chrono::Utc;
use hmac::{Hmac, Mac};
use mairust_common::types::{HookAction, HookResult, HookType};
use mairust_storage::db::DatabasePool;
use mairust_storage::models::{Hook, Message, Plugin};
use mairust_storage::repository::HookRepository;
use reqwest::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Circuit breaker state for a plugin
#[derive(Debug, Clone)]
struct CircuitBreakerState {
    failure_count: u32,
    last_failure: Option<chrono::DateTime<Utc>>,
    is_open: bool,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self {
            failure_count: 0,
            last_failure: None,
            is_open: false,
        }
    }
}

/// Hook execution request sent to plugins
#[derive(Debug, Clone, Serialize)]
pub struct HookRequest {
    pub hook_id: Uuid,
    pub hook_type: String,
    pub message_id: Uuid,
    pub tenant_id: Uuid,
    pub envelope: EnvelopeData,
    pub headers: serde_json::Value,
    pub body_preview: Option<String>,
    pub metadata: serde_json::Value,
}

/// Envelope data for hook requests
#[derive(Debug, Clone, Serialize)]
pub struct EnvelopeData {
    pub from: Option<String>,
    pub to: Vec<String>,
    pub client_ip: Option<String>,
}

/// Hook execution response from plugins
#[derive(Debug, Clone, Deserialize)]
pub struct HookResponse {
    pub action: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub score: Option<f64>,
    pub smtp_code: Option<u16>,
    pub smtp_message: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Hook Manager
pub struct HookManager {
    db_pool: DatabasePool,
    http_client: Client,
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreakerState>>>,
    /// Maximum consecutive failures before circuit opens
    circuit_threshold: u32,
    /// Time to wait before retrying after circuit opens
    circuit_reset_timeout: Duration,
}

impl HookManager {
    /// Create a new hook manager
    pub fn new(db_pool: DatabasePool) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            db_pool,
            http_client,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            circuit_threshold: 5, // Open circuit after 5 consecutive failures
            circuit_reset_timeout: Duration::from_secs(60),
        }
    }

    /// Execute pre_receive hooks
    pub async fn execute_pre_receive(
        &self,
        tenant_id: Uuid,
        envelope: &EnvelopeData,
        headers: &serde_json::Value,
    ) -> Result<Vec<HookResult>> {
        self.execute_hooks(tenant_id, HookType::PreReceive, None, envelope, headers, None)
            .await
    }

    /// Execute post_receive hooks
    pub async fn execute_post_receive(
        &self,
        tenant_id: Uuid,
        message: &Message,
        _raw_data: &[u8],
    ) -> Result<Vec<HookResult>> {
        let envelope = EnvelopeData {
            from: message.from_address.clone(),
            to: serde_json::from_value(message.to_addresses.clone()).unwrap_or_default(),
            client_ip: None,
        };

        self.execute_hooks(
            tenant_id,
            HookType::PostReceive,
            Some(message),
            &envelope,
            &message.headers,
            message.body_preview.as_deref(),
        )
        .await
    }

    /// Execute pre_send hooks
    pub async fn execute_pre_send(
        &self,
        tenant_id: Uuid,
        message: &Message,
    ) -> Result<Vec<HookResult>> {
        let envelope = EnvelopeData {
            from: message.from_address.clone(),
            to: serde_json::from_value(message.to_addresses.clone()).unwrap_or_default(),
            client_ip: None,
        };

        self.execute_hooks(
            tenant_id,
            HookType::PreSend,
            Some(message),
            &envelope,
            &message.headers,
            message.body_preview.as_deref(),
        )
        .await
    }

    /// Execute hooks of a specific type
    async fn execute_hooks(
        &self,
        tenant_id: Uuid,
        hook_type: HookType,
        message: Option<&Message>,
        envelope: &EnvelopeData,
        headers: &serde_json::Value,
        body_preview: Option<&str>,
    ) -> Result<Vec<HookResult>> {
        let hook_repo = HookRepository::new(self.db_pool.clone());

        // Get enabled hooks for this tenant and type, ordered by priority
        let hooks = hook_repo
            .find_by_tenant_and_type(tenant_id, &hook_type.to_string())
            .await?;

        let mut results = Vec::new();

        for hook in hooks {
            if !hook.enabled {
                continue;
            }

            // Check circuit breaker
            if self.is_circuit_open(&hook.plugin_id).await {
                warn!(
                    "Circuit breaker open for plugin {}, skipping hook {}",
                    hook.plugin_id, hook.id
                );
                continue;
            }

            // Execute the hook
            match self
                .execute_single_hook(&hook, message, envelope, headers, body_preview)
                .await
            {
                Ok(result) => {
                    self.record_success(&hook.plugin_id).await;
                    info!(
                        "Hook {} executed successfully: action={:?}",
                        hook.id, result.action
                    );
                    results.push(result);
                }
                Err(e) => {
                    self.record_failure(&hook.plugin_id).await;
                    error!("Hook {} execution failed: {}", hook.id, e);

                    // Apply on_error policy
                    match hook.on_error.as_str() {
                        "reject" => {
                            results.push(HookResult {
                                plugin_id: hook.plugin_id.clone(),
                                action: HookAction::Reject,
                                tags: vec![],
                                score: None,
                                smtp_code: Some(550),
                                smtp_message: Some("Hook error".to_string()),
                                metadata: serde_json::json!({"error": e.to_string()}),
                            });
                        }
                        "tempfail" => {
                            results.push(HookResult {
                                plugin_id: hook.plugin_id.clone(),
                                action: HookAction::Tempfail,
                                tags: vec![],
                                score: None,
                                smtp_code: Some(451),
                                smtp_message: Some("Temporary hook failure".to_string()),
                                metadata: serde_json::json!({"error": e.to_string()}),
                            });
                        }
                        _ => {
                            // "allow" - continue processing
                            debug!("Hook {} failed with on_error=allow, continuing", hook.id);
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Execute a single hook
    async fn execute_single_hook(
        &self,
        hook: &Hook,
        message: Option<&Message>,
        envelope: &EnvelopeData,
        headers: &serde_json::Value,
        body_preview: Option<&str>,
    ) -> Result<HookResult> {
        // Get plugin endpoint
        let plugin = self.get_plugin(&hook.plugin_id).await?;

        let endpoint = plugin
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Plugin {} has no endpoint", hook.plugin_id))?;

        // Build request
        let request = HookRequest {
            hook_id: hook.id,
            hook_type: hook.hook_type.clone(),
            message_id: message.map(|m| m.id).unwrap_or_else(Uuid::nil),
            tenant_id: hook.tenant_id.unwrap_or_else(Uuid::nil),
            envelope: envelope.clone(),
            headers: headers.clone(),
            body_preview: body_preview.map(|s| s.to_string()),
            metadata: hook.config.clone(),
        };

        // Validate endpoint URL to prevent SSRF
        validate_webhook_url(endpoint)?;

        // Calculate timeout
        let timeout = Duration::from_millis(hook.timeout_ms as u64);

        // Serialize request body for HMAC signing
        let request_body = serde_json::to_vec(&request)?;

        // Build request with optional HMAC signature
        let mut http_request = self
            .http_client
            .post(endpoint)
            .header("Content-Type", "application/json")
            .timeout(timeout);

        // Add HMAC-SHA256 signature if plugin has a webhook secret
        if let Some(ref secret) = plugin.webhook_secret {
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                .map_err(|e| anyhow::anyhow!("Invalid HMAC key: {}", e))?;
            mac.update(&request_body);
            let signature = hex::encode(mac.finalize().into_bytes());
            http_request = http_request.header(
                "X-Webhook-Signature",
                format!("sha256={}", signature),
            );
        }

        let response = http_request.body(request_body).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Plugin returned status {}",
                response.status()
            ));
        }

        let hook_response: HookResponse = response.json().await?;

        // Convert response to HookResult
        let action = match hook_response.action.to_lowercase().as_str() {
            "allow" => HookAction::Allow,
            "reject" => HookAction::Reject,
            "tempfail" => HookAction::Tempfail,
            "tag" => HookAction::Tag,
            "quarantine" => HookAction::Quarantine,
            _ => HookAction::Allow,
        };

        Ok(HookResult {
            plugin_id: hook.plugin_id.clone(),
            action,
            tags: hook_response.tags,
            score: hook_response.score,
            smtp_code: hook_response.smtp_code,
            smtp_message: hook_response.smtp_message,
            metadata: hook_response.metadata,
        })
    }

    /// Get plugin information
    async fn get_plugin(&self, plugin_id: &str) -> Result<Plugin> {
        // For now, query from database
        // In production, this could be cached
        let pool = self.db_pool.pool();
        let plugin: Option<Plugin> = sqlx::query_as(
            "SELECT * FROM plugins WHERE id = $1 AND enabled = true"
        )
        .bind(plugin_id)
        .fetch_optional(pool)
        .await?;

        plugin.ok_or_else(|| anyhow::anyhow!("Plugin {} not found or disabled", plugin_id))
    }

    /// Check if circuit breaker is open for a plugin
    async fn is_circuit_open(&self, plugin_id: &str) -> bool {
        let breakers = self.circuit_breakers.read().await;

        if let Some(state) = breakers.get(plugin_id) {
            if state.is_open {
                // Check if reset timeout has passed
                if let Some(last_failure) = state.last_failure {
                    let elapsed = Utc::now()
                        .signed_duration_since(last_failure)
                        .to_std()
                        .unwrap_or(Duration::ZERO);

                    if elapsed < self.circuit_reset_timeout {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Record a successful hook execution
    async fn record_success(&self, plugin_id: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        let state = breakers.entry(plugin_id.to_string()).or_default();

        state.failure_count = 0;
        state.is_open = false;
    }

    /// Record a failed hook execution
    async fn record_failure(&self, plugin_id: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        let state = breakers.entry(plugin_id.to_string()).or_default();

        state.failure_count += 1;
        state.last_failure = Some(Utc::now());

        if state.failure_count >= self.circuit_threshold {
            state.is_open = true;
            warn!(
                "Circuit breaker opened for plugin {} after {} failures",
                plugin_id, state.failure_count
            );
        }
    }

    /// Aggregate hook results and determine final action
    pub fn aggregate_results(&self, results: &[HookResult]) -> HookAction {
        // Priority: Reject > Tempfail > Quarantine > Tag > Allow
        let mut has_quarantine = false;
        let mut has_tag = false;

        for result in results {
            match result.action {
                HookAction::Reject => return HookAction::Reject,
                HookAction::Tempfail => return HookAction::Tempfail,
                HookAction::Quarantine => has_quarantine = true,
                HookAction::Tag => has_tag = true,
                HookAction::Allow => {}
            }
        }

        if has_quarantine {
            HookAction::Quarantine
        } else if has_tag {
            HookAction::Tag
        } else {
            HookAction::Allow
        }
    }

    /// Collect all tags from hook results
    pub fn collect_tags(&self, results: &[HookResult]) -> Vec<String> {
        results
            .iter()
            .flat_map(|r| r.tags.clone())
            .collect()
    }

    /// Calculate aggregate spam score
    pub fn aggregate_spam_score(&self, results: &[HookResult]) -> Option<f64> {
        let scores: Vec<f64> = results.iter().filter_map(|r| r.score).collect();

        if scores.is_empty() {
            None
        } else {
            // Use maximum score
            Some(scores.iter().cloned().fold(f64::MIN, f64::max))
        }
    }
}

/// Validate a webhook URL to prevent SSRF attacks.
///
/// Rejects URLs targeting private/internal IP ranges, loopback addresses,
/// link-local addresses, and non-HTTP(S) schemes.
fn validate_webhook_url(url_str: &str) -> Result<()> {
    let url = Url::parse(url_str)
        .map_err(|e| anyhow::anyhow!("Invalid webhook URL: {}", e))?;

    // Only allow HTTP and HTTPS schemes
    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(anyhow::anyhow!(
                "Webhook URL scheme '{}' is not allowed. Only http and https are permitted.",
                scheme
            ));
        }
    }

    // Resolve the hostname to check for private IP ranges
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Webhook URL has no host"))?;

    // Block obviously internal hostnames
    let lower_host = host.to_lowercase();
    if lower_host == "localhost"
        || lower_host.ends_with(".local")
        || lower_host.ends_with(".internal")
        || lower_host == "metadata.google.internal"
        || lower_host == "169.254.169.254"
    {
        return Err(anyhow::anyhow!(
            "Webhook URL host '{}' is not allowed (internal/private address)",
            host
        ));
    }

    // Check if the host is an IP address and block private ranges
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(&ip) {
            return Err(anyhow::anyhow!(
                "Webhook URL IP '{}' is not allowed (private/internal range)",
                ip
            ));
        }
    }

    Ok(())
}

/// Check if an IP address is in a private/reserved range
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()              // 127.0.0.0/8
                || ipv4.is_private()         // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || ipv4.is_link_local()      // 169.254.0.0/16
                || ipv4.is_broadcast()       // 255.255.255.255
                || ipv4.is_unspecified()      // 0.0.0.0
                || ipv4.octets()[0] == 100 && (ipv4.octets()[1] & 0xC0) == 64  // 100.64.0.0/10 (CGNAT)
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()              // ::1
                || ipv6.is_unspecified()     // ::
                // fc00::/7 (ULA)
                || (ipv6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 (link-local)
                || (ipv6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_results() {
        // Test with mock manager - would need actual db in real tests
    }

    #[test]
    fn test_collect_tags() {
        let results = vec![
            HookResult {
                plugin_id: "p1".to_string(),
                action: HookAction::Tag,
                tags: vec!["spam".to_string(), "bulk".to_string()],
                score: Some(5.0),
                smtp_code: None,
                smtp_message: None,
                metadata: serde_json::json!({}),
            },
            HookResult {
                plugin_id: "p2".to_string(),
                action: HookAction::Allow,
                tags: vec!["newsletter".to_string()],
                score: None,
                smtp_code: None,
                smtp_message: None,
                metadata: serde_json::json!({}),
            },
        ];

        // Would test with actual manager instance
        let tags: Vec<String> = results
            .iter()
            .flat_map(|r| r.tags.clone())
            .collect();

        assert_eq!(tags, vec!["spam", "bulk", "newsletter"]);
    }
}
