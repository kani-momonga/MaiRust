//! Plugin System
//!
//! Provides the plugin infrastructure for MaiRust, including
//! AI categorization and external integrations.

mod categorization;
mod manager;
mod types;

pub use categorization::{
    AiCategorizationPlugin, CategorizationInput, CategorizationOutput,
    DefaultAiCategorizer, RuleBasedCategorizer,
};
pub use manager::{PluginManager, PluginManagerConfig};
pub use types::{
    Plugin, PluginCapability, PluginContext, PluginError, PluginEvent,
    PluginHealth, PluginInfo, PluginResult, PluginStatus,
};
