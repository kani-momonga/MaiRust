//! MaiRust Storage - Database and file storage abstraction
//!
//! This crate provides storage abstraction for MaiRust,
//! supporting PostgreSQL, SQLite, and file/S3 storage.

pub mod db;
pub mod file;
pub mod models;
pub mod repository;

pub use db::{Database, DatabasePool};
pub use file::{create_storage, FileStorage, LocalStorage, MessageStorage};
pub use models::*;
pub use repository::*;
