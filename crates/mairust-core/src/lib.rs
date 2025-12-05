//! MaiRust Core - SMTP/IMAP/POP3 server and mail processing
//!
//! This crate provides the core mail server functionality for MaiRust,
//! including message reception, hook execution, queue management, and plugin system.

pub mod email_auth;
pub mod hooks;
pub mod imap;
pub mod plugins;
pub mod policy;
pub mod pop3;
pub mod queue;
pub mod search;
pub mod smtp;
pub mod spam;

pub use email_auth::{AuthenticationResult, DkimResult, DkimSigner, DmarcResult, SpfResult};
pub use hooks::HookManager;
pub use imap::{ImapConfig, ImapServer};
pub use plugins::{PluginManager, PluginManagerConfig, AiCategorizationPlugin, CategorizationInput, CategorizationOutput};
pub use policy::{PolicyContext, PolicyEngine, PolicyEvaluation, PolicyEvaluationResult, PolicyMatch};
pub use pop3::{Pop3Config, Pop3Server};
pub use queue::QueueManager;
pub use search::{MeilisearchClient, MeilisearchConfig, MessageDocument, MessageIndexer, MessageSearchHit, SearchOptions, SearchResult};
pub use smtp::SmtpServer;
pub use spam::{RspamdClient, RspamdConfig, SpamAction, SpamCheckResult, SpamFilter};
