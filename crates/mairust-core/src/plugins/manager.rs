//! Plugin Manager
//!
//! Manages plugin lifecycle, registration, and execution.

use super::categorization::{AiCategorizationPlugin, CategorizationInput, CategorizationOutput, DefaultAiCategorizer};
use super::types::{Plugin, PluginContext, PluginError, PluginHealth, PluginInfo, PluginProtocol, PluginResult, PluginStatus};
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

/// Plugin manifest format (plugin.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin unique identifier
    pub id: String,
    /// Plugin display name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: Option<String>,
    /// Plugin author
    pub author: Option<String>,
    /// Plugin type (categorization, filter, webhook, etc.)
    #[serde(default = "default_plugin_type")]
    pub plugin_type: String,
    /// Minimum MaiRust version required
    pub min_version: Option<String>,
    /// Maximum MaiRust version supported
    pub max_version: Option<String>,
    /// Plugin endpoint URL (for webhook plugins)
    pub endpoint: Option<String>,
    /// Plugin permissions required
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn default_plugin_type() -> String {
    "generic".to_string()
}

impl PluginManifest {
    /// Check if plugin is compatible with current MaiRust version
    pub fn is_compatible(&self) -> bool {
        // For now, accept all plugins
        // In the future, implement version checking
        true
    }
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

        // Load external plugins from plugin_dir
        if let Some(plugin_dir) = self.config.plugin_dir.clone() {
            if let Err(e) = self.load_plugins_from_directory(&plugin_dir).await {
                warn!("Failed to load plugins from directory: {}", e);
            }
        }

        Ok(())
    }

    /// Load plugins from a directory
    async fn load_plugins_from_directory(&mut self, plugin_dir: &str) -> PluginResult<()> {
        let path = std::path::Path::new(plugin_dir);
        if !path.exists() {
            debug!("Plugin directory does not exist: {}", plugin_dir);
            return Ok(());
        }

        info!("Loading plugins from directory: {}", plugin_dir);

        let entries = match std::fs::read_dir(path) {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read plugin directory: {}", e);
                return Ok(());
            }
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                // Check for plugin.toml in subdirectory
                let plugin_toml = entry_path.join("plugin.toml");
                if plugin_toml.exists() {
                    if let Err(e) = self.load_plugin_from_manifest(&plugin_toml).await {
                        warn!("Failed to load plugin from {:?}: {}", plugin_toml, e);
                    }
                }
            } else if entry_path.extension().map_or(false, |e| e == "toml") {
                // Load standalone plugin.toml files
                if let Err(e) = self.load_plugin_from_manifest(&entry_path).await {
                    warn!("Failed to load plugin from {:?}: {}", entry_path, e);
                }
            }
        }

        Ok(())
    }

    /// Load a plugin from its manifest file
    async fn load_plugin_from_manifest(&mut self, manifest_path: &std::path::Path) -> PluginResult<()> {
        let content = std::fs::read_to_string(manifest_path)
            .map_err(|e| PluginError::Internal(format!("Failed to read manifest: {}", e)))?;

        let manifest: PluginManifest = toml::from_str(&content)
            .map_err(|e| PluginError::Internal(format!("Failed to parse manifest: {}", e)))?;

        // Check if plugin is compatible
        if !manifest.is_compatible() {
            warn!("Plugin {} is not compatible with this version", manifest.id);
            return Ok(());
        }

        let protocol = if let Some(ref endpoint) = manifest.endpoint {
            PluginProtocol::Http { endpoint: endpoint.clone() }
        } else {
            PluginProtocol::Native
        };

        let plugin_info = PluginInfo {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            homepage: None,
            capabilities: Vec::new(),
            protocol,
        };

        // Register the plugin
        self.plugins.write().await.insert(
            manifest.id.clone(),
            PluginEntry {
                info: plugin_info,
                status: PluginStatus::Active,
                enabled: true,
                error_count: 0,
                last_error: None,
                last_success: Some(Utc::now()),
            },
        );

        info!("Loaded plugin: {} v{}", manifest.name, manifest.version);
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

    /// Install a plugin from a package or manifest path
    pub async fn install_plugin(&mut self, package_path: &str) -> PluginResult<PluginInfo> {
        let path = std::path::Path::new(package_path);

        if !path.exists() {
            return Err(PluginError::NotFound(format!(
                "Plugin package not found: {}",
                package_path
            )));
        }

        // If it's a directory, look for plugin.toml inside
        let manifest_path = if path.is_dir() {
            path.join("plugin.toml")
        } else if path.extension().map_or(false, |e| e == "toml") {
            path.to_path_buf()
        } else {
            return Err(PluginError::Internal(
                "Plugin package must be a directory with plugin.toml or a .toml file".to_string(),
            ));
        };

        if !manifest_path.exists() {
            return Err(PluginError::NotFound(format!(
                "Plugin manifest not found: {:?}",
                manifest_path
            )));
        }

        // Read and parse the manifest
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Internal(format!("Failed to read manifest: {}", e)))?;

        let manifest: PluginManifest = toml::from_str(&content)
            .map_err(|e| PluginError::Internal(format!("Failed to parse manifest: {}", e)))?;

        // Check if plugin is compatible
        if !manifest.is_compatible() {
            return Err(PluginError::Internal(format!(
                "Plugin {} is not compatible with this version",
                manifest.id
            )));
        }

        // Check if already installed
        if self.plugins.read().await.contains_key(&manifest.id) {
            return Err(PluginError::Internal(format!(
                "Plugin {} is already installed",
                manifest.id
            )));
        }

        // Copy to plugin directory if configured
        if let Some(ref plugin_dir) = self.config.plugin_dir {
            let target_dir = std::path::Path::new(plugin_dir).join(&manifest.id);
            if !target_dir.exists() {
                std::fs::create_dir_all(&target_dir)
                    .map_err(|e| PluginError::Internal(format!("Failed to create plugin directory: {}", e)))?;
            }

            // Copy manifest
            let target_manifest = target_dir.join("plugin.toml");
            std::fs::copy(&manifest_path, &target_manifest)
                .map_err(|e| PluginError::Internal(format!("Failed to copy manifest: {}", e)))?;

            info!("Installed plugin files to {:?}", target_dir);
        }

        let protocol = if let Some(ref endpoint) = manifest.endpoint {
            PluginProtocol::Http { endpoint: endpoint.clone() }
        } else {
            PluginProtocol::Native
        };

        let plugin_info = PluginInfo {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            homepage: None,
            capabilities: Vec::new(),
            protocol,
        };

        // Register the plugin
        self.plugins.write().await.insert(
            manifest.id.clone(),
            PluginEntry {
                info: plugin_info.clone(),
                status: PluginStatus::Active,
                enabled: true,
                error_count: 0,
                last_error: None,
                last_success: Some(Utc::now()),
            },
        );

        info!("Installed plugin: {} v{}", manifest.name, manifest.version);

        Ok(plugin_info)
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
