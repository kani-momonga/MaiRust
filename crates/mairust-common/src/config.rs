//! Configuration for MaiRust

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Database configuration
    pub database: DatabaseConfig,

    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,

    /// SMTP configuration
    #[serde(default)]
    pub smtp: SmtpConfig,

    /// API configuration
    #[serde(default)]
    pub api: ApiConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,

    /// TLS configuration
    pub tls: Option<TlsConfig>,

    /// Meilisearch configuration for full-text search
    #[serde(default)]
    pub meilisearch: MeilisearchConfig,

    /// IMAP configuration
    #[serde(default)]
    pub imap: ImapConfig,

    /// POP3 configuration
    #[serde(default)]
    pub pop3: Pop3Config,

    /// Web UI configuration
    #[serde(default)]
    pub web: WebConfig,

    /// Plugin configuration
    #[serde(default)]
    pub plugins: PluginConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Hostname
    #[serde(default = "default_hostname")]
    pub hostname: String,

    /// Bind address
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hostname: default_hostname(),
            bind_address: default_bind_address(),
        }
    }
}

fn default_hostname() -> String {
    "localhost".to_string()
}

fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database backend: "postgres" or "sqlite"
    #[serde(default = "default_db_backend")]
    pub backend: String,

    /// Database URL (for postgres)
    pub url: Option<String>,

    /// Database path (for sqlite)
    pub path: Option<PathBuf>,

    /// Maximum connections
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Minimum connections
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
}

fn default_db_backend() -> String {
    "postgres".to_string()
}

fn default_max_connections() -> u32 {
    20
}

fn default_min_connections() -> u32 {
    5
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend: "fs" or "s3"
    #[serde(default = "default_storage_backend")]
    pub backend: String,

    /// Base path for local filesystem storage
    #[serde(default = "default_storage_path")]
    pub path: PathBuf,

    /// S3 configuration
    pub s3: Option<S3Config>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: default_storage_backend(),
            path: default_storage_path(),
            s3: None,
        }
    }
}

fn default_storage_backend() -> String {
    "fs".to_string()
}

fn default_storage_path() -> PathBuf {
    PathBuf::from("/var/lib/mairust/mail")
}

/// S3 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    /// S3 bucket name
    pub bucket: String,

    /// AWS region
    pub region: String,

    /// Custom endpoint (for MinIO, etc.)
    pub endpoint: Option<String>,

    /// Access key ID
    pub access_key_id: Option<String>,

    /// Secret access key
    pub secret_access_key: Option<String>,

    /// Multipart upload threshold in MB
    #[serde(default = "default_multipart_threshold")]
    pub multipart_threshold_mb: u32,
}

fn default_multipart_threshold() -> u32 {
    8
}

/// SMTP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// Hostname for SMTP banner
    #[serde(default = "default_hostname")]
    pub hostname: String,

    /// Bind host
    #[serde(default = "default_smtp_host")]
    pub host: String,

    /// SMTP port (inbound)
    #[serde(default = "default_smtp_port")]
    pub port: u16,

    /// Submission port
    #[serde(default = "default_submission_port")]
    pub submission_port: u16,

    /// Maximum message size in bytes
    pub max_message_size: Option<usize>,

    /// Maximum recipients per message
    #[serde(default = "default_max_recipients")]
    pub max_recipients: usize,

    /// Maximum concurrent connections
    pub max_connections: Option<usize>,

    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_secs: u64,

    /// Enable TLS
    pub tls_enabled: Option<bool>,

    /// Require authentication
    pub auth_required: Option<bool>,

    /// Require TLS for authentication
    #[serde(default = "default_require_tls_for_auth")]
    pub require_tls_for_auth: bool,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            hostname: default_hostname(),
            host: default_smtp_host(),
            port: default_smtp_port(),
            submission_port: default_submission_port(),
            max_message_size: Some(default_max_message_size()),
            max_recipients: default_max_recipients(),
            max_connections: Some(100),
            connection_timeout_secs: default_connection_timeout(),
            tls_enabled: Some(false),
            auth_required: Some(false),
            require_tls_for_auth: default_require_tls_for_auth(),
        }
    }
}

fn default_smtp_host() -> String {
    "0.0.0.0".to_string()
}

fn default_smtp_port() -> u16 {
    25
}

fn default_submission_port() -> u16 {
    587
}

fn default_max_message_size() -> usize {
    25 * 1024 * 1024 // 25 MB
}

fn default_max_recipients() -> usize {
    100
}

fn default_connection_timeout() -> u64 {
    300
}

fn default_require_tls_for_auth() -> bool {
    true
}

/// API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API port
    #[serde(default = "default_api_port")]
    pub port: u16,

    /// Enable Swagger UI
    #[serde(default = "default_enable_swagger")]
    pub enable_swagger: bool,

    /// CORS allowed origins
    #[serde(default)]
    pub cors_origins: Vec<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            port: default_api_port(),
            enable_swagger: default_enable_swagger(),
            cors_origins: Vec::new(),
        }
    }
}

fn default_api_port() -> u16 {
    8080
}

fn default_enable_swagger() -> bool {
    true
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log format: "json" or "text"
    #[serde(default = "default_log_format")]
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "json".to_string()
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to certificate file
    pub cert_path: PathBuf,

    /// Path to private key file
    pub key_path: PathBuf,
}

/// Meilisearch configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeilisearchConfig {
    /// Enable Meilisearch integration
    #[serde(default)]
    pub enabled: bool,

    /// Meilisearch server URL
    #[serde(default = "default_meilisearch_url")]
    pub url: String,

    /// API key for authentication
    pub api_key: Option<String>,

    /// Request timeout in seconds
    #[serde(default = "default_meilisearch_timeout")]
    pub timeout_secs: u64,

    /// Index name for messages
    #[serde(default = "default_messages_index")]
    pub messages_index: String,
}

impl Default for MeilisearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: default_meilisearch_url(),
            api_key: None,
            timeout_secs: default_meilisearch_timeout(),
            messages_index: default_messages_index(),
        }
    }
}

fn default_meilisearch_url() -> String {
    "http://localhost:7700".to_string()
}

fn default_meilisearch_timeout() -> u64 {
    30
}

fn default_messages_index() -> String {
    "messages".to_string()
}

/// IMAP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    /// Enable IMAP server
    #[serde(default)]
    pub enabled: bool,

    /// IMAP server bind address
    #[serde(default = "default_imap_bind")]
    pub bind: String,

    /// Enable STARTTLS
    #[serde(default)]
    pub starttls: bool,

    /// Session timeout in minutes
    #[serde(default = "default_imap_timeout")]
    pub timeout_minutes: i64,

    /// Maximum concurrent connections
    #[serde(default = "default_imap_max_connections")]
    pub max_connections: usize,
}

impl Default for ImapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_imap_bind(),
            starttls: false,
            timeout_minutes: default_imap_timeout(),
            max_connections: default_imap_max_connections(),
        }
    }
}

fn default_imap_bind() -> String {
    "0.0.0.0:143".to_string()
}

fn default_imap_timeout() -> i64 {
    30
}

fn default_imap_max_connections() -> usize {
    1000
}

/// POP3 server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pop3Config {
    /// Enable POP3 server
    #[serde(default)]
    pub enabled: bool,

    /// POP3 server bind address
    #[serde(default = "default_pop3_bind")]
    pub bind: String,

    /// Enable STARTTLS
    #[serde(default)]
    pub starttls: bool,

    /// Session timeout in minutes
    #[serde(default = "default_pop3_timeout")]
    pub timeout_minutes: i64,

    /// Maximum concurrent connections
    #[serde(default = "default_pop3_max_connections")]
    pub max_connections: usize,
}

impl Default for Pop3Config {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_pop3_bind(),
            starttls: false,
            timeout_minutes: default_pop3_timeout(),
            max_connections: default_pop3_max_connections(),
        }
    }
}

fn default_pop3_bind() -> String {
    "0.0.0.0:110".to_string()
}

fn default_pop3_timeout() -> i64 {
    10
}

fn default_pop3_max_connections() -> usize {
    500
}

/// Web UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Enable Web UI
    #[serde(default)]
    pub enabled: bool,

    /// Web UI server bind address
    #[serde(default = "default_web_bind")]
    pub bind: String,

    /// API base URL for frontend
    #[serde(default = "default_web_api_url")]
    pub api_url: String,

    /// Enable debug mode
    #[serde(default)]
    pub debug: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_web_bind(),
            api_url: default_web_api_url(),
            debug: false,
        }
    }
}

fn default_web_bind() -> String {
    "0.0.0.0:8081".to_string()
}

fn default_web_api_url() -> String {
    "/api/v1".to_string()
}

/// Plugin system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Enable plugin system
    #[serde(default = "default_plugins_enabled")]
    pub enabled: bool,

    /// Plugin directory path
    #[serde(default = "default_plugin_dir")]
    pub plugin_dir: Option<String>,

    /// Plugin execution timeout in milliseconds
    #[serde(default = "default_plugin_timeout")]
    pub timeout_ms: u64,

    /// Enable built-in categorizer
    #[serde(default = "default_enable_categorizer")]
    pub enable_categorizer: bool,

    /// AI service endpoint for categorization
    pub ai_endpoint: Option<String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: default_plugins_enabled(),
            plugin_dir: default_plugin_dir(),
            timeout_ms: default_plugin_timeout(),
            enable_categorizer: default_enable_categorizer(),
            ai_endpoint: None,
        }
    }
}

fn default_plugins_enabled() -> bool {
    true
}

fn default_plugin_dir() -> Option<String> {
    Some("/var/lib/mairust/plugins".to_string())
}

fn default_plugin_timeout() -> u64 {
    5000
}

fn default_enable_categorizer() -> bool {
    true
}

impl Config {
    /// Load configuration from file
    pub fn from_file(path: &std::path::Path) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::Error::Config(format!("Failed to read config file: {}", e)))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| crate::Error::Config(format!("Failed to parse config: {}", e)))?;

        Ok(config)
    }

    /// Load configuration from environment and file
    pub fn load() -> crate::Result<Self> {
        // Try to load from default locations
        let paths = [
            std::path::PathBuf::from("./config.yaml"),
            std::path::PathBuf::from("./config.toml"),
            std::path::PathBuf::from("/etc/mairust/config.yaml"),
            std::path::PathBuf::from("/etc/mairust/config.toml"),
        ];

        for path in paths {
            if path.exists() {
                return Self::from_file(&path);
            }
        }

        Err(crate::Error::Config(
            "No configuration file found".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let server = ServerConfig::default();
        assert_eq!(server.hostname, "localhost");
        assert_eq!(server.bind_address, "0.0.0.0");

        let smtp = SmtpConfig::default();
        assert_eq!(smtp.port, 25);
        assert_eq!(smtp.submission_port, 587);
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
[server]
hostname = "mail.example.com"

[database]
backend = "postgres"
url = "postgres://localhost/mairust"

[storage]
backend = "fs"
path = "/data/mail"

[smtp]
port = 25
submission_port = 587
"#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.server.hostname, "mail.example.com");
        assert_eq!(config.database.backend, "postgres");
        assert_eq!(config.smtp.port, 25);
    }
}
