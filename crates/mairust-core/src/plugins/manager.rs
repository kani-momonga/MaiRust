//! Plugin Manager
//!
//! Manages plugin lifecycle, registration, and execution.

use super::categorization::{AiCategorizationPlugin, CategorizationInput, CategorizationOutput, DefaultAiCategorizer};
use super::types::{Plugin, PluginContext, PluginError, PluginHealth, PluginInfo, PluginResult, PluginStatus};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Plugin manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerConfig {
    /// Enable plugin system
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Maximum plugin execution time (ms)
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Enable built-in categorizer
    #[serde(default = "default_enable_categorizer")]
    pub enable_categorizer: bool,
    /// AI service endpoint (if any)
    pub ai_endpoint: Option<String>,
    /// Plugin directory
    pub plugin_dir: Option<String>,
}

fn default_enabled() -> bool {
    true
}

fn default_timeout() -> u64 {
    5000
}

fn default_enable_categorizer() -> bool {
    true
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_ms: 5000,
            enable_categorizer: true,
            ai_endpoint: None,
            plugin_dir: None,
        }
    }
}

/// Registered plugin entry
struct PluginEntry {
    info: PluginInfo,
    status: PluginStatus,
    enabled: bool,
    error_count: u32,
    last_error: Option<String>,
    last_success: Option<chrono::DateTime<Utc>>,
}

/// Plugin Manager
pub struct PluginManager {
    config: PluginManagerConfig,
    /// Registered plugins (by ID)
    plugins: RwLock<HashMap<String, PluginEntry>>,
    /// Active categorization plugin
    categorizer: Option<Arc<RwLock<Box<dyn AiCategorizationPlugin + Send + Sync>>>>,
    /// Category ID mappings
    category_ids: HashMap<String, Uuid>,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new(config: PluginManagerConfig) -> Self {
        Self {
            config,
            plugins: RwLock::new(HashMap::new()),
            categorizer: None,
            category_ids: HashMap::new(),
        }
    }

    /// Set category ID mappings
    pub fn with_category_ids(mut self, ids: HashMap<String, Uuid>) -> Self {
        self.category_ids = ids;
        self
    }

    /// Initialize the plugin manager
    pub async fn initialize(&mut self) -> PluginResult<()> {
        if !self.config.enabled {
            info!("Plugin system disabled");
            return Ok(());
        }

        info!("Initializing plugin manager");

        // Initialize built-in categorizer if enabled
        if self.config.enable_categorizer {
            let mut categorizer = if let Some(ref endpoint) = self.config.ai_endpoint {
                Box::new(
                    DefaultAiCategorizer::new()
                        .with_endpoint(endpoint.clone())
                        .with_category_ids(self.category_ids.clone()),
                ) as Box<dyn AiCategorizationPlugin + Send + Sync>
            } else {
                Box::new(
                    DefaultAiCategorizer::new()
                        .with_category_ids(self.category_ids.clone()),
                ) as Box<dyn AiCategorizationPlugin + Send + Sync>
            };

            categorizer.initialize().await?;

            let info = categorizer.info().clone();
            self.categorizer = Some(Arc::new(RwLock::new(categorizer)));

            // Register the categorizer
            self.plugins.write().await.insert(
                info.id.clone(),
                PluginEntry {
                    info,
                    status: PluginStatus::Active,
                    enabled: true,
                    error_count: 0,
                    last_error: None,
                    last_success: Some(Utc::now()),
                },
            );

            info!("Built-in AI categorizer initialized");
        }

        // TODO: Load external plugins from plugin_dir

        Ok(())
    }

    /// Shutdown the plugin manager
    pub async fn shutdown(&mut self) -> PluginResult<()> {
        info!("Shutting down plugin manager");

        if let Some(ref categorizer) = self.categorizer {
            categorizer.write().await.shutdown().await?;
        }

        // TODO: Shutdown external plugins

        Ok(())
    }

    /// Get list of registered plugins
    pub async fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .read()
            .await
            .values()
            .map(|e| e.info.clone())
            .collect()
    }

    /// Get plugin health
    pub async fn get_plugin_health(&self, plugin_id: &str) -> PluginResult<PluginHealth> {
        let plugins = self.plugins.read().await;

        let entry = plugins
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        Ok(PluginHealth {
            status: entry.status,
            last_check: Utc::now(),
            message: entry.last_error.clone(),
            error_count: entry.error_count,
            success_count: 0, // TODO: Track this
            avg_response_ms: 0.0, // TODO: Track this
        })
    }

    /// Enable/disable a plugin
    pub async fn set_plugin_enabled(&self, plugin_id: &str, enabled: bool) -> PluginResult<()> {
        let mut plugins = self.plugins.write().await;

        let entry = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        entry.enabled = enabled;
        entry.status = if enabled {
            PluginStatus::Active
        } else {
            PluginStatus::Disabled
        };

        info!("Plugin {} {}", plugin_id, if enabled { "enabled" } else { "disabled" });

        Ok(())
    }

    /// Categorize a message
    pub async fn categorize_message(
        &self,
        ctx: &PluginContext,
        input: &CategorizationInput,
    ) -> PluginResult<CategorizationOutput> {
        let categorizer = self
            .categorizer
            .as_ref()
            .ok_or_else(|| PluginError::NotFound("categorizer".to_string()))?;

        let start = std::time::Instant::now();

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.timeout_ms),
            categorizer.read().await.categorize(ctx, input),
        )
        .await
        .map_err(|_| PluginError::Timeout("Categorization timeout".to_string()))?;

        let elapsed = start.elapsed().as_millis() as u64;
        debug!("Categorization completed in {}ms", elapsed);

        result
    }

    /// Batch categorize messages
    pub async fn categorize_messages(
        &self,
        ctx: &PluginContext,
        inputs: &[CategorizationInput],
    ) -> PluginResult<Vec<CategorizationOutput>> {
        let categorizer = self
            .categorizer
            .as_ref()
            .ok_or_else(|| PluginError::NotFound("categorizer".to_string()))?;

        let start = std::time::Instant::now();

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.timeout_ms * inputs.len() as u64),
            categorizer.read().await.categorize_batch(ctx, inputs),
        )
        .await
        .map_err(|_| PluginError::Timeout("Batch categorization timeout".to_string()))?;

        let elapsed = start.elapsed().as_millis() as u64;
        debug!(
            "Batch categorization of {} messages completed in {}ms",
            inputs.len(),
            elapsed
        );

        result
    }

    /// Provide feedback for categorization learning
    pub async fn categorization_feedback(
        &self,
        ctx: &PluginContext,
        message_id: Uuid,
        correct_category_id: Uuid,
    ) -> PluginResult<()> {
        if let Some(ref categorizer) = self.categorizer {
            categorizer
                .read()
                .await
                .feedback(ctx, message_id, correct_category_id)
                .await?;
        }

        Ok(())
    }

    /// Install a plugin from a package
    pub async fn install_plugin(&mut self, _package_path: &str) -> PluginResult<PluginInfo> {
        // TODO: Implement plugin installation
        // 1. Verify package signature
        // 2. Extract and parse plugin.toml
        // 3. Check compatibility
        // 4. Install files to plugin directory
        // 5. Register plugin
        Err(PluginError::Internal("Plugin installation not yet implemented".to_string()))
    }

    /// Uninstall a plugin
    pub async fn uninstall_plugin(&mut self, plugin_id: &str) -> PluginResult<()> {
        // Check if plugin exists
        let plugins = self.plugins.read().await;
        if !plugins.contains_key(plugin_id) {
            return Err(PluginError::NotFound(plugin_id.to_string()));
        }
        drop(plugins);

        // Built-in plugins cannot be uninstalled
        if plugin_id.starts_with("mairust.builtin.") {
            return Err(PluginError::PermissionDenied(
                "Cannot uninstall built-in plugins".to_string(),
            ));
        }

        // TODO: Implement plugin uninstallation
        // 1. Shutdown plugin
        // 2. Remove files
        // 3. Unregister

        self.plugins.write().await.remove(plugin_id);

        info!("Plugin {} uninstalled", plugin_id);

        Ok(())
    }

    /// Get all plugin health statuses
    pub async fn health_check_all(&self) -> HashMap<String, PluginHealth> {
        let mut results = HashMap::new();

        for (id, _entry) in self.plugins.read().await.iter() {
            if let Ok(health) = self.get_plugin_health(id).await {
                results.insert(id.clone(), health);
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_manager_init() {
        let config = PluginManagerConfig::default();
        let mut manager = PluginManager::new(config);

        manager.initialize().await.unwrap();

        let plugins = manager.list_plugins().await;
        assert!(!plugins.is_empty());

        manager.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_categorize_message() {
        let config = PluginManagerConfig::default();
        let mut manager = PluginManager::new(config);

        manager.initialize().await.unwrap();

        let ctx = PluginContext::new(Uuid::new_v4());
        let input = CategorizationInput {
            message_id: Uuid::new_v4(),
            from_address: Some("test@example.com".to_string()),
            to_addresses: vec!["user@example.com".to_string()],
            subject: Some("Test message".to_string()),
            body_preview: Some("This is a test".to_string()),
            headers: HashMap::new(),
            spam_score: None,
            tags: vec![],
        };

        let result = manager.categorize_message(&ctx, &input).await.unwrap();
        assert!(!result.category_name.is_empty());

        manager.shutdown().await.unwrap();
    }
}
