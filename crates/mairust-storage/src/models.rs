//! Database models

use chrono::{DateTime, Utc};
use mairust_common::types::{
    DomainAliasId, DomainId, HookId, HookType, MailboxId, MessageFlags, MessageId, PolicyId,
    TenantId, UserId, UserRole,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Tenant model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Tenant {
    pub id: TenantId,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan: String,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Domain model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Domain {
    pub id: DomainId,
    pub tenant_id: TenantId,
    pub name: String,
    pub verified: bool,
    pub dkim_selector: Option<String>,
    pub dkim_private_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub tenant_id: TenantId,
    pub email: String,
    pub password_hash: String,
    pub name: Option<String>,
    pub role: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Mailbox model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Mailbox {
    pub id: MailboxId,
    pub tenant_id: TenantId,
    pub domain_id: DomainId,
    pub user_id: Option<UserId>,
    pub address: String,
    pub display_name: Option<String>,
    pub quota_bytes: Option<i64>,
    pub used_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Message model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub tenant_id: TenantId,
    pub mailbox_id: MailboxId,
    pub message_id_header: Option<String>,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub to_addresses: serde_json::Value,
    pub cc_addresses: Option<serde_json::Value>,
    pub headers: serde_json::Value,
    pub body_preview: Option<String>,
    pub body_size: i64,
    pub has_attachments: bool,
    pub storage_path: String,
    pub seen: bool,
    pub answered: bool,
    pub flagged: bool,
    pub deleted: bool,
    pub draft: bool,
    pub spam_score: Option<f64>,
    pub tags: serde_json::Value,
    pub metadata: serde_json::Value,
    pub received_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl Message {
    /// Get message flags
    pub fn flags(&self) -> MessageFlags {
        MessageFlags {
            seen: self.seen,
            answered: self.answered,
            flagged: self.flagged,
            deleted: self.deleted,
            draft: self.draft,
        }
    }

    /// Get tags as a vector
    pub fn tags_vec(&self) -> Vec<String> {
        serde_json::from_value(self.tags.clone()).unwrap_or_default()
    }
}

/// Hook model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Hook {
    pub id: HookId,
    pub tenant_id: Option<TenantId>,
    pub name: String,
    pub hook_type: String,
    pub plugin_id: String,
    pub enabled: bool,
    pub priority: i32,
    pub timeout_ms: i32,
    pub on_timeout: String,
    pub on_error: String,
    pub filter_config: serde_json::Value,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Hook {
    /// Get hook type enum
    pub fn hook_type_enum(&self) -> Option<HookType> {
        match self.hook_type.as_str() {
            "pre_receive" => Some(HookType::PreReceive),
            "post_receive" => Some(HookType::PostReceive),
            "pre_send" => Some(HookType::PreSend),
            "pre_delivery" => Some(HookType::PreDelivery),
            _ => None,
        }
    }
}

/// Plugin model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub plugin_type: String,
    pub protocol: String,
    pub endpoint: Option<String>,
    pub permissions: serde_json::Value,
    pub enabled: bool,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// API key model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub user_id: Option<UserId>,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub scopes: serde_json::Value,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Session model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Audit log model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: uuid::Uuid,
    pub tenant_id: Option<TenantId>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Job queue model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Job {
    pub id: uuid::Uuid,
    pub queue: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Create tenant input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTenant {
    pub name: String,
    pub slug: String,
    pub plan: Option<String>,
    pub settings: Option<serde_json::Value>,
}

/// Create domain input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDomain {
    pub tenant_id: TenantId,
    pub name: String,
}

/// Create user input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUser {
    pub tenant_id: TenantId,
    pub email: String,
    pub password: String,
    pub name: Option<String>,
    pub role: UserRole,
}

/// Create mailbox input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMailbox {
    pub tenant_id: TenantId,
    pub domain_id: DomainId,
    pub user_id: Option<UserId>,
    pub address: String,
    pub display_name: Option<String>,
    pub quota_bytes: Option<i64>,
}

/// Create message input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessage {
    pub tenant_id: TenantId,
    pub mailbox_id: MailboxId,
    pub message_id_header: Option<String>,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub to_addresses: Vec<String>,
    pub cc_addresses: Option<Vec<String>>,
    pub headers: serde_json::Value,
    pub body_preview: Option<String>,
    pub body_size: i64,
    pub has_attachments: bool,
    pub storage_path: String,
    pub received_at: DateTime<Utc>,
}

// ============================================================================
// Phase 2: Multi-domain support enhancements
// ============================================================================

/// Domain alias model - maps alias domain to primary domain
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DomainAlias {
    pub id: DomainAliasId,
    pub tenant_id: TenantId,
    pub alias_domain: String,
    pub primary_domain_id: DomainId,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Domain settings model - extended configuration for domains
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DomainSettings {
    pub domain_id: DomainId,
    /// Enable catch-all for unknown addresses
    pub catch_all_enabled: bool,
    /// Mailbox to receive catch-all emails
    pub catch_all_mailbox_id: Option<MailboxId>,
    /// Maximum message size for this domain (bytes)
    pub max_message_size: Option<i64>,
    /// Maximum recipients per message
    pub max_recipients: Option<i32>,
    /// Rate limit: messages per hour
    pub rate_limit_per_hour: Option<i32>,
    /// Require TLS for inbound connections
    pub require_tls_inbound: bool,
    /// Require TLS for outbound connections
    pub require_tls_outbound: bool,
    /// Custom SPF policy mode
    pub spf_policy: String,
    /// Custom DMARC policy mode
    pub dmarc_policy: String,
    /// Additional settings as JSON
    pub extra_settings: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

impl Default for DomainSettings {
    fn default() -> Self {
        Self {
            domain_id: uuid::Uuid::nil(),
            catch_all_enabled: false,
            catch_all_mailbox_id: None,
            max_message_size: None,
            max_recipients: None,
            rate_limit_per_hour: None,
            require_tls_inbound: false,
            require_tls_outbound: false,
            spf_policy: "neutral".to_string(),
            dmarc_policy: "none".to_string(),
            extra_settings: serde_json::json!({}),
            updated_at: Utc::now(),
        }
    }
}

/// Create domain alias input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDomainAlias {
    pub tenant_id: TenantId,
    pub alias_domain: String,
    pub primary_domain_id: DomainId,
}

/// Update domain settings input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDomainSettings {
    pub catch_all_enabled: Option<bool>,
    pub catch_all_mailbox_id: Option<MailboxId>,
    pub max_message_size: Option<i64>,
    pub max_recipients: Option<i32>,
    pub rate_limit_per_hour: Option<i32>,
    pub require_tls_inbound: Option<bool>,
    pub require_tls_outbound: Option<bool>,
    pub spf_policy: Option<String>,
    pub dmarc_policy: Option<String>,
    pub extra_settings: Option<serde_json::Value>,
}

// ============================================================================
// Phase 2: Advanced Policy System
// ============================================================================

/// Policy condition types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyConditionType {
    SenderDomain,
    SenderAddress,
    RecipientDomain,
    RecipientAddress,
    SubjectContains,
    HeaderExists,
    HeaderValue,
    MessageSize,
    AttachmentType,
    SpamScore,
    ClientIp,
    TimeOfDay,
}

/// Policy action types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyActionType {
    Allow,
    Reject,
    Tempfail,
    Quarantine,
    Tag,
    Redirect,
    AddHeader,
    ModifySubject,
    RateLimit,
    RequireTls,
}

/// Policy rule model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: PolicyId,
    pub tenant_id: Option<TenantId>,
    pub domain_id: Option<DomainId>,
    pub name: String,
    pub description: Option<String>,
    /// Policy type: inbound, outbound, or both
    pub policy_type: String,
    pub priority: i32,
    pub enabled: bool,
    /// Conditions as JSON array
    pub conditions: serde_json::Value,
    /// Actions as JSON array
    pub actions: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create policy rule input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePolicyRule {
    pub tenant_id: Option<TenantId>,
    pub domain_id: Option<DomainId>,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub priority: i32,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
}

/// Policy condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCondition {
    pub condition_type: PolicyConditionType,
    pub operator: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub negate: bool,
}

/// Policy action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyAction {
    pub action_type: PolicyActionType,
    pub parameters: serde_json::Value,
}
