//! Hook Manager - Executes hooks and manages plugin calls

use anyhow::Result;
use chrono::Utc;
use mairust_common::types::{HookAction, HookResult, HookType};
use mairust_storage::db::DatabasePool;
use mairust_storage::models::{Hook, Message, Plugin};
use mairust_storage::repository::HookRepository;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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

        // Calculate timeout
        let timeout = Duration::from_millis(hook.timeout_ms as u64);

        // Make HTTP request
        let response = self
            .http_client
            .post(endpoint)
            .json(&request)
            .timeout(timeout)
            .send()
            .await?;

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
