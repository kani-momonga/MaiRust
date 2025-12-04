//! Error types for MaiRust

use thiserror::Error;

/// Main error type for MaiRust
#[derive(Error, Debug)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Hook error: {0}")]
    Hook(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Result type alias for MaiRust
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Returns the HTTP status code for this error
    pub fn status_code(&self) -> u16 {
        match self {
            Error::Config(_) => 500,
            Error::Database(_) => 500,
            Error::Storage(_) => 500,
            Error::Smtp(_) => 500,
            Error::Auth(_) => 401,
            Error::Validation(_) => 422,
            Error::NotFound(_) => 404,
            Error::PermissionDenied(_) => 403,
            Error::RateLimitExceeded => 429,
            Error::Plugin(_) => 500,
            Error::Hook(_) => 500,
            Error::Internal(_) => 500,
            Error::Other(_) => 500,
        }
    }

    /// Returns the error code string
    pub fn code(&self) -> &'static str {
        match self {
            Error::Config(_) => "CONFIG_ERROR",
            Error::Database(_) => "DATABASE_ERROR",
            Error::Storage(_) => "STORAGE_ERROR",
            Error::Smtp(_) => "SMTP_ERROR",
            Error::Auth(_) => "UNAUTHORIZED",
            Error::Validation(_) => "VALIDATION_ERROR",
            Error::NotFound(_) => "NOT_FOUND",
            Error::PermissionDenied(_) => "FORBIDDEN",
            Error::RateLimitExceeded => "RATE_LIMITED",
            Error::Plugin(_) => "PLUGIN_ERROR",
            Error::Hook(_) => "HOOK_ERROR",
            Error::Internal(_) => "INTERNAL_ERROR",
            Error::Other(_) => "INTERNAL_ERROR",
        }
    }
}
