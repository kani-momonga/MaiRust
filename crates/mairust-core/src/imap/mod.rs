//! IMAP4 Server Module (Read-Only)
//!
//! Implements a read-only IMAP4 server for mail access.
//! This implementation supports reading mail but not modifying it.
//!
//! Supported commands:
//! - CAPABILITY, NOOP, LOGOUT
//! - LOGIN, AUTHENTICATE (PLAIN)
//! - LIST, LSUB, SELECT, EXAMINE
//! - FETCH, SEARCH
//! - CLOSE, CHECK

pub mod command;
pub mod parser;
pub mod response;
pub mod server;
pub mod session;

pub use server::{ImapConfig, ImapServer};
