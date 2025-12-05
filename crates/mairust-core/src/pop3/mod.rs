//! POP3 Server Module
//!
//! Implements the Post Office Protocol version 3 (POP3) for mail retrieval.

mod command;
mod response;
mod server;
mod session;

pub use server::{Pop3Config, Pop3Server};
