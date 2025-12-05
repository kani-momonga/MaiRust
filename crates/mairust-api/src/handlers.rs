//! API request handlers

pub mod admin;
pub mod domains;
pub mod domain_aliases;
pub mod domain_settings;
pub mod health;
pub mod hooks;
pub mod mailboxes;
pub mod messages;
pub mod policies;
pub mod search;
pub mod send;
pub mod tenants;
pub mod users;

pub use health::*;
