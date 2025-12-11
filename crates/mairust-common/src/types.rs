//! Common types for MaiRust

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for tenants
pub type TenantId = Uuid;

/// Unique identifier for domains
pub type DomainId = Uuid;

/// Unique identifier for users
pub type UserId = Uuid;

/// Unique identifier for mailboxes
pub type MailboxId = Uuid;

/// Unique identifier for messages
pub type MessageId = Uuid;

/// Unique identifier for plugins
pub type PluginId = String;

/// Unique identifier for hooks
pub type HookId = Uuid;

/// Unique identifier for domain aliases
pub type DomainAliasId = Uuid;

/// Unique identifier for policies
pub type PolicyId = Uuid;

/// Unique identifier for campaigns
pub type CampaignId = Uuid;

/// Unique identifier for recipient lists
pub type RecipientListId = Uuid;

/// Unique identifier for recipients
pub type RecipientId = Uuid;

/// Unique identifier for scheduled messages
pub type ScheduledMessageId = Uuid;

/// Email address
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EmailAddress {
    pub local: String,
    pub domain: String,
}

impl EmailAddress {
    /// Create a new email address
    pub fn new(local: impl Into<String>, domain: impl Into<String>) -> Self {
        Self {
            local: local.into(),
            domain: domain.into(),
        }
    }

    /// Parse an email address from a string
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(2, '@').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            Some(Self::new(parts[0], parts[1]))
        } else {
            None
        }
    }

    /// Get the full email address as a string
    pub fn to_string(&self) -> String {
        format!("{}@{}", self.local, self.domain)
    }
}

impl std::fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.local, self.domain)
    }
}

impl std::str::FromStr for EmailAddress {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| crate::Error::Validation("Invalid email address".to_string()))
    }
}

/// Message envelope (SMTP level)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// Sender (MAIL FROM)
    pub from: Option<EmailAddress>,

    /// Recipients (RCPT TO)
    pub to: Vec<EmailAddress>,

    /// Client IP address
    pub client_ip: Option<String>,

    /// HELO/EHLO hostname
    pub helo: Option<String>,
}

/// Message headers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageHeaders {
    pub subject: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub cc: Option<String>,
    pub date: Option<String>,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub content_type: Option<String>,

    /// Additional headers
    #[serde(flatten)]
    pub other: std::collections::HashMap<String, String>,
}

/// Message body information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBody {
    /// Text preview (first 4KB)
    pub preview: Option<String>,

    /// Total size in bytes
    pub size: usize,

    /// Whether the message has attachments
    pub has_attachments: bool,

    /// Attachment metadata
    #[serde(default)]
    pub attachments: Vec<AttachmentMeta>,
}

/// Attachment metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    /// Filename
    pub filename: Option<String>,

    /// Content type
    pub content_type: String,

    /// Size in bytes
    pub size: usize,

    /// Content ID (for inline attachments)
    pub content_id: Option<String>,
}

/// Message flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MessageFlags {
    pub seen: bool,
    pub answered: bool,
    pub flagged: bool,
    pub deleted: bool,
    pub draft: bool,
}

/// Hook types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookType {
    PreReceive,
    PostReceive,
    PreSend,
    PreDelivery,
}

impl std::fmt::Display for HookType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookType::PreReceive => write!(f, "pre_receive"),
            HookType::PostReceive => write!(f, "post_receive"),
            HookType::PreSend => write!(f, "pre_send"),
            HookType::PreDelivery => write!(f, "pre_delivery"),
        }
    }
}

/// Hook action results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookAction {
    Allow,
    Reject,
    Tempfail,
    Tag,
    Quarantine,
}

/// Hook execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    pub plugin_id: PluginId,
    pub action: HookAction,
    #[serde(default)]
    pub tags: Vec<String>,
    pub score: Option<f64>,
    pub smtp_code: Option<u16>,
    pub smtp_message: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Tenant status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantStatus {
    Active,
    Suspended,
    Deleted,
}

/// User role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    SuperAdmin,
    TenantAdmin,
    DomainAdmin,
    User,
}

/// Pagination cursor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationCursor {
    pub cursor: Option<String>,
    pub limit: usize,
}

impl Default for PaginationCursor {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 50,
        }
    }
}

/// Paginated response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paginated<T> {
    pub data: Vec<T>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

/// Timestamp wrapper
pub type Timestamp = DateTime<Utc>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_address_parse() {
        let email = EmailAddress::parse("user@example.com").unwrap();
        assert_eq!(email.local, "user");
        assert_eq!(email.domain, "example.com");
        assert_eq!(email.to_string(), "user@example.com");
    }

    #[test]
    fn test_email_address_invalid() {
        assert!(EmailAddress::parse("invalid").is_none());
        assert!(EmailAddress::parse("@example.com").is_none());
        assert!(EmailAddress::parse("user@").is_none());
    }

    #[test]
    fn test_hook_type_display() {
        assert_eq!(HookType::PreReceive.to_string(), "pre_receive");
        assert_eq!(HookType::PostReceive.to_string(), "post_receive");
    }
}
