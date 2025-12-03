//! MaiRust Core - SMTP server and mail processing
//!
//! This crate provides the core SMTP server functionality for MaiRust,
//! including message reception, hook execution, and queue management.

pub mod hooks;
pub mod queue;
pub mod smtp;

pub use hooks::HookManager;
pub use queue::QueueManager;
pub use smtp::SmtpServer;
