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

// ============================================================================
// Phase 3: Message Threading
// ============================================================================

/// Thread model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Thread {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub mailbox_id: MailboxId,
    pub subject: Option<String>,
    pub participant_addresses: serde_json::Value,
    pub message_count: i32,
    pub unread_count: i32,
    pub first_message_at: Option<DateTime<Utc>>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_id: Option<MessageId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create thread input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThread {
    pub tenant_id: TenantId,
    pub mailbox_id: MailboxId,
    pub subject: Option<String>,
}

// ============================================================================
// Phase 3: Enhanced Tagging System
// ============================================================================

/// Tag model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Tag {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create tag input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTag {
    pub tenant_id: TenantId,
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// Update tag input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTag {
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// Message tag relationship
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MessageTag {
    pub message_id: MessageId,
    pub tag_id: uuid::Uuid,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Phase 3: AI Categorization
// ============================================================================

/// Category model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Category {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub priority: i32,
    pub auto_rules: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create category input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCategory {
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub priority: Option<i32>,
    pub auto_rules: Option<serde_json::Value>,
}

/// AI categorization result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorizationResult {
    pub category_id: uuid::Uuid,
    pub confidence: f32,
    pub summary: Option<String>,
    pub metadata: serde_json::Value,
}

// ============================================================================
// Phase 3: Plugin System
// ============================================================================

/// Plugin event log
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PluginEvent {
    pub id: uuid::Uuid,
    pub plugin_id: String,
    pub tenant_id: Option<TenantId>,
    pub event_type: String,
    pub message_id: Option<MessageId>,
    pub input_data: Option<serde_json::Value>,
    pub output_data: Option<serde_json::Value>,
    pub status: String,
    pub error_message: Option<String>,
    pub execution_time_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// Plugin configuration per tenant
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PluginConfig {
    pub plugin_id: String,
    pub tenant_id: TenantId,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Mailbox subscription
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MailboxSubscription {
    pub user_id: UserId,
    pub mailbox_id: MailboxId,
    pub subscribed: bool,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Phase 4: Scheduled Email Sending
// ============================================================================

/// Recipient list model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RecipientList {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub recipient_count: i32,
    pub active_count: i32,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create recipient list input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecipientList {
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Update recipient list input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecipientList {
    pub name: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Recipient status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipientStatus {
    Active,
    Unsubscribed,
    Bounced,
    Complained,
}

impl std::fmt::Display for RecipientStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecipientStatus::Active => write!(f, "active"),
            RecipientStatus::Unsubscribed => write!(f, "unsubscribed"),
            RecipientStatus::Bounced => write!(f, "bounced"),
            RecipientStatus::Complained => write!(f, "complained"),
        }
    }
}

impl std::str::FromStr for RecipientStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(RecipientStatus::Active),
            "unsubscribed" => Ok(RecipientStatus::Unsubscribed),
            "bounced" => Ok(RecipientStatus::Bounced),
            "complained" => Ok(RecipientStatus::Complained),
            _ => Err(format!("Invalid recipient status: {}", s)),
        }
    }
}

/// Recipient model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Recipient {
    pub id: uuid::Uuid,
    pub recipient_list_id: uuid::Uuid,
    pub email: String,
    pub name: Option<String>,
    pub status: String,
    pub attributes: serde_json::Value,
    pub subscribed_at: DateTime<Utc>,
    pub unsubscribed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Recipient {
    /// Get status enum
    pub fn status_enum(&self) -> Option<RecipientStatus> {
        self.status.parse().ok()
    }
}

/// Create recipient input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecipient {
    pub recipient_list_id: uuid::Uuid,
    pub email: String,
    pub name: Option<String>,
    pub attributes: Option<serde_json::Value>,
}

/// Update recipient input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecipient {
    pub name: Option<String>,
    pub status: Option<RecipientStatus>,
    pub attributes: Option<serde_json::Value>,
}

/// Campaign status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    Draft,
    Scheduled,
    Sending,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl std::fmt::Display for CampaignStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CampaignStatus::Draft => write!(f, "draft"),
            CampaignStatus::Scheduled => write!(f, "scheduled"),
            CampaignStatus::Sending => write!(f, "sending"),
            CampaignStatus::Paused => write!(f, "paused"),
            CampaignStatus::Completed => write!(f, "completed"),
            CampaignStatus::Cancelled => write!(f, "cancelled"),
            CampaignStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for CampaignStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(CampaignStatus::Draft),
            "scheduled" => Ok(CampaignStatus::Scheduled),
            "sending" => Ok(CampaignStatus::Sending),
            "paused" => Ok(CampaignStatus::Paused),
            "completed" => Ok(CampaignStatus::Completed),
            "cancelled" => Ok(CampaignStatus::Cancelled),
            "failed" => Ok(CampaignStatus::Failed),
            _ => Err(format!("Invalid campaign status: {}", s)),
        }
    }
}

/// Campaign model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Campaign {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub subject: String,
    pub from_address: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub recipient_list_id: Option<uuid::Uuid>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub rate_limit_per_hour: i32,
    pub rate_limit_per_minute: i32,
    pub status: String,
    pub total_recipients: i32,
    pub sent_count: i32,
    pub delivered_count: i32,
    pub bounced_count: i32,
    pub failed_count: i32,
    pub opened_count: i32,
    pub clicked_count: i32,
    pub tags: serde_json::Value,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Campaign {
    /// Get status enum
    pub fn status_enum(&self) -> Option<CampaignStatus> {
        self.status.parse().ok()
    }

    /// Get tags as a vector
    pub fn tags_vec(&self) -> Vec<String> {
        serde_json::from_value(self.tags.clone()).unwrap_or_default()
    }

    /// Calculate progress percentage
    pub fn progress_percentage(&self) -> f64 {
        if self.total_recipients == 0 {
            0.0
        } else {
            (self.sent_count as f64 / self.total_recipients as f64) * 100.0
        }
    }
}

/// Create campaign input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCampaign {
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub subject: String,
    pub from_address: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub recipient_list_id: Option<uuid::Uuid>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub rate_limit_per_hour: Option<i32>,
    pub rate_limit_per_minute: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

/// Update campaign input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCampaign {
    pub name: Option<String>,
    pub description: Option<String>,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub recipient_list_id: Option<uuid::Uuid>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub rate_limit_per_hour: Option<i32>,
    pub rate_limit_per_minute: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

/// Scheduled message status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledMessageStatus {
    Pending,
    Processing,
    Sent,
    Delivered,
    Bounced,
    Failed,
    Cancelled,
}

impl std::fmt::Display for ScheduledMessageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduledMessageStatus::Pending => write!(f, "pending"),
            ScheduledMessageStatus::Processing => write!(f, "processing"),
            ScheduledMessageStatus::Sent => write!(f, "sent"),
            ScheduledMessageStatus::Delivered => write!(f, "delivered"),
            ScheduledMessageStatus::Bounced => write!(f, "bounced"),
            ScheduledMessageStatus::Failed => write!(f, "failed"),
            ScheduledMessageStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for ScheduledMessageStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(ScheduledMessageStatus::Pending),
            "processing" => Ok(ScheduledMessageStatus::Processing),
            "sent" => Ok(ScheduledMessageStatus::Sent),
            "delivered" => Ok(ScheduledMessageStatus::Delivered),
            "bounced" => Ok(ScheduledMessageStatus::Bounced),
            "failed" => Ok(ScheduledMessageStatus::Failed),
            "cancelled" => Ok(ScheduledMessageStatus::Cancelled),
            _ => Err(format!("Invalid scheduled message status: {}", s)),
        }
    }
}

/// Scheduled message model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ScheduledMessage {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub campaign_id: Option<uuid::Uuid>,
    pub recipient_id: Option<uuid::Uuid>,
    pub batch_id: Option<uuid::Uuid>,
    pub from_address: String,
    pub to_address: String,
    pub subject: String,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub headers: serde_json::Value,
    pub scheduled_at: DateTime<Utc>,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub message_id: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub bounced_at: Option<DateTime<Utc>>,
    pub bounce_type: Option<String>,
    pub bounce_reason: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ScheduledMessage {
    /// Get status enum
    pub fn status_enum(&self) -> Option<ScheduledMessageStatus> {
        self.status.parse().ok()
    }

    /// Check if can retry
    pub fn can_retry(&self) -> bool {
        self.attempts < self.max_attempts
            && matches!(
                self.status_enum(),
                Some(ScheduledMessageStatus::Pending) | Some(ScheduledMessageStatus::Failed)
            )
    }
}

/// Create scheduled message input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateScheduledMessage {
    pub tenant_id: TenantId,
    pub campaign_id: Option<uuid::Uuid>,
    pub recipient_id: Option<uuid::Uuid>,
    pub batch_id: Option<uuid::Uuid>,
    pub from_address: String,
    pub to_address: String,
    pub subject: String,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub scheduled_at: DateTime<Utc>,
    pub max_attempts: Option<i32>,
    pub metadata: Option<serde_json::Value>,
}

/// Tenant rate limits model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TenantRateLimit {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub per_minute: i32,
    pub per_hour: i32,
    pub per_day: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for TenantRateLimit {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::nil(),
            tenant_id: uuid::Uuid::nil(),
            per_minute: 100,
            per_hour: 5000,
            per_day: 50000,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Create/update tenant rate limit input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertTenantRateLimit {
    pub tenant_id: TenantId,
    pub per_minute: Option<i32>,
    pub per_hour: Option<i32>,
    pub per_day: Option<i32>,
    pub enabled: Option<bool>,
}

/// Rate limit counter model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RateLimitCounter {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub window_type: String,
    pub window_start: DateTime<Utc>,
    pub count: i32,
    pub limit_value: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Unsubscribe source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsubscribeSource {
    Manual,
    Link,
    Bounce,
    Complaint,
}

impl std::fmt::Display for UnsubscribeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnsubscribeSource::Manual => write!(f, "manual"),
            UnsubscribeSource::Link => write!(f, "link"),
            UnsubscribeSource::Bounce => write!(f, "bounce"),
            UnsubscribeSource::Complaint => write!(f, "complaint"),
        }
    }
}

/// Unsubscribe model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Unsubscribe {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub email: String,
    pub source: String,
    pub campaign_id: Option<uuid::Uuid>,
    pub reason: Option<String>,
    pub unsubscribed_at: DateTime<Utc>,
}

/// Create unsubscribe input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUnsubscribe {
    pub tenant_id: TenantId,
    pub email: String,
    pub source: UnsubscribeSource,
    pub campaign_id: Option<uuid::Uuid>,
    pub reason: Option<String>,
}

/// Campaign statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignStats {
    pub campaign_id: uuid::Uuid,
    pub status: String,
    pub total_recipients: i32,
    pub sent: i32,
    pub delivered: i32,
    pub bounced: i32,
    pub failed: i32,
    pub opened: i32,
    pub clicked: i32,
    pub unsubscribed: i32,
    pub progress_percentage: f64,
    pub estimated_completion: Option<DateTime<Utc>>,
    pub current_rate: i32,
    pub rate_limit_per_hour: i32,
}
