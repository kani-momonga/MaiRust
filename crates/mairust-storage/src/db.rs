//! Database connection and pool management

use mairust_common::config::DatabaseConfig;
use mairust_common::{Error, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

/// Database pool wrapper
#[derive(Clone)]
pub struct DatabasePool {
    pool: PgPool,
}

impl DatabasePool {
    /// Create a new database pool from configuration
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let url = Self::build_url(config)?;

        info!(
            backend = %config.backend,
            "Connecting to database"
        );

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&url)
            .await
            .map_err(|e| Error::Database(format!("Failed to connect: {}", e)))?;

        info!("Database connection established");

        Ok(Self { pool })
    }

    /// Build database URL from configuration
    fn build_url(config: &DatabaseConfig) -> Result<String> {
        match config.backend.as_str() {
            "postgres" => config
                .url
                .clone()
                .ok_or_else(|| Error::Config("Database URL required for PostgreSQL".to_string())),
            other => Err(Error::Config(format!(
                "Unsupported database backend: {} (Phase 1 only supports PostgreSQL)",
                other
            ))),
        }
    }

    /// Get the underlying pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        info!("Running database migrations");

        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Migration failed: {}", e)))?;

        info!("Database migrations completed");
        Ok(())
    }

    /// Check database health
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Health check failed: {}", e)))?;
        Ok(())
    }
}

/// Database trait for type-safe queries
pub trait Database: Send + Sync {
    /// Get the pool
    fn pool(&self) -> &PgPool;
}

impl Database for DatabasePool {
    fn pool(&self) -> &PgPool {
        &self.pool
    }
}
