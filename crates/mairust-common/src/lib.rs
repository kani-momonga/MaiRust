//! MaiRust Common - Shared types and utilities
//!
//! This crate provides common types, configuration, and utilities
//! shared across all MaiRust components.

pub mod config;
pub mod error;
pub mod types;

pub use config::Config;
pub use error::{Error, Result};
