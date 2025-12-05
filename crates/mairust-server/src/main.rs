//! MaiRust - Mail server entry point

use anyhow::Result;
use mairust_common::config::Config;
use mairust_core::{
    HookManager, ImapServer, PluginManager, PluginManagerConfig, Pop3Config, Pop3Server,
    QueueManager, SmtpServer,
};
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

    // Initialize plugin manager
    let plugin_config = PluginManagerConfig {
        enabled: config.plugins.enabled,
        timeout_ms: config.plugins.timeout_ms,
        enable_categorizer: config.plugins.enable_categorizer,
        ai_endpoint: config.plugins.ai_endpoint.clone(),
        plugin_dir: config.plugins.plugin_dir.clone(),
    };
    let mut plugin_manager = PluginManager::new(plugin_config);
    if config.plugins.enabled {
        plugin_manager.initialize().await?;
        info!("Plugin manager initialized");
    }
    let plugin_manager = Arc::new(tokio::sync::RwLock::new(plugin_manager));

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

    // Initialize SMTP server
    let smtp_server = Arc::new(SmtpServer::new(
        config.smtp.clone(),
        db_pool.clone(),
        file_storage.clone(),
        hook_manager.clone(),
        queue_manager.clone(),
    ));

    info!(
        "Starting SMTP server on {}:{} (SMTP) and {}:{} (Submission)",
        config.smtp.host, config.smtp.port, config.smtp.host, config.smtp.submission_port
    );

    // Start SMTP server
    let smtp_handle = {
        let smtp_server = smtp_server.clone();
        tokio::spawn(async move {
            if let Err(e) = smtp_server.run_dual_port().await {
                tracing::error!("SMTP server error: {}", e);
            }
        })
    };

    // Start IMAP server if enabled
    let imap_handle = if config.imap.enabled {
        let imap_config = mairust_core::imap::ImapConfig {
            bind: config.imap.bind.clone(),
            starttls: config.imap.starttls,
            timeout_minutes: config.imap.timeout_minutes,
            max_connections: config.imap.max_connections,
            storage_path: config.storage.path.clone(),
        };
        let imap_server = ImapServer::new(imap_config, db_pool.clone());
        info!("Starting IMAP server on {}", config.imap.bind);

        Some(tokio::spawn(async move {
            if let Err(e) = imap_server.run().await {
                tracing::error!("IMAP server error: {}", e);
            }
        }))
    } else {
        info!("IMAP server disabled");
        None
    };

    // Start POP3 server if enabled
    let pop3_handle = if config.pop3.enabled {
        let pop3_config = Pop3Config {
            bind: config.pop3.bind.clone(),
            starttls: config.pop3.starttls,
            timeout_minutes: config.pop3.timeout_minutes,
            max_connections: config.pop3.max_connections,
            server_name: config.server.hostname.clone(),
            storage_path: config.storage.path.clone(),
        };
        let pop3_server = Pop3Server::new(pop3_config, db_pool.clone());
        info!("Starting POP3 server on {}", config.pop3.bind);

        Some(tokio::spawn(async move {
            if let Err(e) = pop3_server.run().await {
                tracing::error!("POP3 server error: {}", e);
            }
        }))
    } else {
        info!("POP3 server disabled");
        None
    };

    // Start API server
    let api_handle = {
        let db_pool = db_pool.clone();
        let api_port = config.api.port;
        tokio::spawn(async move {
            let app = mairust_api::create_router(db_pool);
            let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", api_port))
                .await
                .expect("Failed to bind API server");
            info!("Starting API server on port {}", api_port);
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("API server error: {}", e);
            }
        })
    };

    // Start Web UI server if enabled
    let web_handle = if config.web.enabled {
        let web_config = mairust_web::WebConfig {
            bind: config.web.bind.clone(),
            api_url: config.web.api_url.clone(),
            debug: config.web.debug,
        };
        let db_pool = db_pool.clone();
        info!("Starting Web UI server on {}", config.web.bind);

        Some(tokio::spawn(async move {
            if let Err(e) = mairust_web::run(web_config, db_pool).await {
                tracing::error!("Web UI server error: {}", e);
            }
        }))
    } else {
        info!("Web UI server disabled");
        None
    };

    info!("MaiRust server started successfully");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received");

    // Cleanup
    smtp_handle.abort();
    queue_handle.abort();
    api_handle.abort();

    if let Some(handle) = imap_handle {
        handle.abort();
    }
    if let Some(handle) = pop3_handle {
        handle.abort();
    }
    if let Some(handle) = web_handle {
        handle.abort();
    }

    // Shutdown plugin manager
    {
        let mut pm = plugin_manager.write().await;
        let _ = pm.shutdown().await;
    }

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
