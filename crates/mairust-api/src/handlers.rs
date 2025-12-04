//! API request handlers

pub mod domains;
pub mod health;
pub mod hooks;
pub mod mailboxes;
pub mod messages;
pub mod send;
pub mod tenants;
pub mod users;

pub use health::*;
