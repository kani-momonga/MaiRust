//! MaiRust API - REST API server
//!
//! This crate provides the REST API server for MaiRust,
//! including authentication, message management, and admin endpoints.

pub mod auth;
pub mod handlers;
pub mod routes;

pub use routes::create_router;
