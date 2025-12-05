//! Full-text search module with Meilisearch integration
//!
//! This module provides full-text search capabilities for email messages
//! using Meilisearch as the search backend.

pub mod client;
pub mod indexer;

pub use client::{MeilisearchClient, MeilisearchConfig, SearchResult};
pub use indexer::{MessageDocument, MessageIndexer, MessageSearchHit, SearchOptions};
