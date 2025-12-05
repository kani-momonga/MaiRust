//! Plugin Types
//!
//! Core types for the plugin system.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// Plugin error types
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Plugin timeout: {0}")]
    Timeout(String),
    #[error("Plugin configuration error: {0}")]
    ConfigError(String),
    #[error("Plugin communication error: {0}")]
    CommunicationError(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Plugin result type
pub type PluginResult<T> = Result<T, PluginError>;

/// Plugin capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    /// Read email headers
    ReadHeaders,
    /// Read email body (preview)
    ReadBodyPreview,
    /// Read email body (full)
    ReadBodyFull,
    /// Read attachments (metadata)
    ReadAttachmentsMeta,
    /// Read attachments (full)
    ReadAttachmentsFull,
    /// Write tags
    WriteTags,
    /// Write flags
    WriteFlags,
    /// Write category
    WriteCategory,
    /// Move messages
    MoveMessages,
    /// Send notifications
    SendNotifications,
    /// Access external network
    NetworkAccess,
}

/// Plugin status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    /// Plugin is active and running
    Active,
    /// Plugin is installed but disabled
    Disabled,
    /// Plugin is in error state
    Error,
    /// Plugin is being initialized
    Initializing,
    /// Plugin is stopped
    Stopped,
}

/// Plugin health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHealth {
    pub status: PluginStatus,
    pub last_check: DateTime<Utc>,
    pub message: Option<String>,
    pub error_count: u32,
    pub success_count: u64,
    pub avg_response_ms: f64,
}

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub capabilities: Vec<PluginCapability>,
    pub protocol: PluginProtocol,
}

/// Plugin communication protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginProtocol {
    /// HTTP/HTTPS REST API
    Http { endpoint: String },
    /// gRPC
    Grpc { endpoint: String },
    /// Standard input/output
    Stdio { command: String, args: Vec<String> },
    /// Built-in (native Rust)
    Native,
}

/// Plugin execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginContext {
    pub tenant_id: Uuid,
    pub user_id: Option<Uuid>,
    pub request_id: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PluginContext {
    /// Create a new plugin context
    pub fn new(tenant_id: Uuid) -> Self {
        Self {
            tenant_id,
            user_id: None,
            request_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Set user ID
    pub fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }
}

/// Plugin event for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEvent {
    pub id: Uuid,
    pub plugin_id: String,
    pub tenant_id: Option<Uuid>,
    pub event_type: String,
    pub message_id: Option<Uuid>,
    pub status: String,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub error_message: Option<String>,
    pub execution_time_ms: u64,
    pub timestamp: DateTime<Utc>,
}

/// Plugin trait
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Get plugin information
    fn info(&self) -> &PluginInfo;

    /// Initialize the plugin
    async fn initialize(&mut self) -> PluginResult<()>;

    /// Shutdown the plugin
    async fn shutdown(&mut self) -> PluginResult<()>;

    /// Check plugin health
    async fn health_check(&self) -> PluginResult<PluginHealth>;

    /// Get plugin status
    fn status(&self) -> PluginStatus;
}
