//! MaiRust - Mail server entry point

use anyhow::Result;
use mairust_common::config::Config;
use mairust_core::{HookManager, QueueManager, SmtpServer};
use mairust_storage::{db::DatabasePool, file::LocalStorage};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    init_logging();

    info!("Starting MaiRust mail server...");

    // Load configuration
    let config = Config::load()?;

    // Initialize database
    let db_pool = DatabasePool::new(&config.database).await?;
    info!("Database connection established");

    // Run migrations
    db_pool.migrate().await?;
    info!("Database migrations completed");

    // Initialize file storage
    let file_storage = Arc::new(LocalStorage::new(&config.storage)?);

    // Initialize hook manager
    let hook_manager = Arc::new(HookManager::new(db_pool.clone()));

    // Initialize queue manager
    let queue_manager = Arc::new(QueueManager::new(
        db_pool.clone(),
        file_storage.clone(),
        hook_manager.clone(),
    ));

    // Start queue processor
    let queue_handle = {
        let queue_manager = queue_manager.clone();
        tokio::spawn(async move {
            queue_manager.run().await;
        })
    };

    // Initialize and start SMTP server
    let smtp_server = SmtpServer::new(
        config.smtp.clone(),
        db_pool.clone(),
        file_storage.clone(),
        hook_manager.clone(),
        queue_manager.clone(),
    );

    info!(
        "Starting SMTP server on {}:{}",
        config.smtp.host, config.smtp.port
    );

    // Run SMTP server (blocking)
    smtp_server.run().await?;

    // Cleanup
    queue_handle.abort();
    info!("MaiRust server shutdown complete");

    Ok(())
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,mairust=debug"));

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_level(true))
        .with(filter)
        .init();
}
