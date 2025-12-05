//! IMAP4 Server Module
//!
//! Implements a full-featured IMAP4rev1 server for mail access.
//! This implementation supports both reading and writing operations.
//!
//! Supported commands:
//! - CAPABILITY, NOOP, LOGOUT
//! - LOGIN, AUTHENTICATE (PLAIN)
//! - LIST, LSUB, SELECT, EXAMINE, STATUS
//! - FETCH, SEARCH
//! - CLOSE, CHECK
//! - CREATE, DELETE, RENAME, SUBSCRIBE, UNSUBSCRIBE (mailbox management)
//! - STORE, COPY, MOVE, EXPUNGE, APPEND (message operations)
//! - IDLE, NAMESPACE (extensions)

pub mod command;
pub mod parser;
pub mod response;
pub mod server;
pub mod session;

pub use server::{ImapConfig, ImapServer};
