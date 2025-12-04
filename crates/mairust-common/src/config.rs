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
