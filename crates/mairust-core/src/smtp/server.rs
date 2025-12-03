//! SMTP server implementation

use crate::hooks::HookManager;
use crate::queue::QueueManager;
use crate::smtp::SmtpHandler;
use anyhow::Result;
use mairust_common::config::SmtpConfig;
use mairust_storage::db::DatabasePool;
use mairust_storage::file::FileStorage;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

/// SMTP Server
pub struct SmtpServer<S: FileStorage> {
    config: SmtpConfig,
    db_pool: DatabasePool,
    file_storage: Arc<S>,
    hook_manager: Arc<HookManager>,
    queue_manager: Arc<QueueManager<S>>,
    connection_semaphore: Arc<Semaphore>,
}

impl<S: FileStorage + Send + Sync + 'static> SmtpServer<S> {
    /// Create a new SMTP server
    pub fn new(
        config: SmtpConfig,
        db_pool: DatabasePool,
        file_storage: Arc<S>,
        hook_manager: Arc<HookManager>,
        queue_manager: Arc<QueueManager<S>>,
    ) -> Self {
        let max_connections = config.max_connections.unwrap_or(100);
        Self {
            config,
            db_pool,
            file_storage,
            hook_manager,
            queue_manager,
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
        }
    }

    /// Run the SMTP server
    pub async fn run(&self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("SMTP server listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    // Acquire semaphore permit
                    let permit = match self.connection_semaphore.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            warn!("Max connections reached, rejecting {}", peer_addr);
                            continue;
                        }
                    };

                    let handler = SmtpHandler::new(
                        self.config.clone(),
                        self.db_pool.clone(),
                        self.file_storage.clone(),
                        self.hook_manager.clone(),
                        self.queue_manager.clone(),
                        peer_addr,
                    );

                    tokio::spawn(async move {
                        if let Err(e) = handler.handle(stream).await {
                            error!("SMTP session error from {}: {}", peer_addr, e);
                        }
                        drop(permit);
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}
