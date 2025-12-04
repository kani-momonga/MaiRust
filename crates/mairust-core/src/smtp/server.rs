//! SMTP server implementation

use crate::hooks::HookManager;
use crate::queue::QueueManager;
use crate::smtp::tls::create_tls_acceptor;
use crate::smtp::SmtpHandler;
use anyhow::Result;
use mairust_common::config::{Config, SmtpConfig};
use mairust_storage::db::DatabasePool;
use mairust_storage::file::FileStorage;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info, warn};

/// SMTP service type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmtpServiceType {
    /// Port 25 - inbound mail reception
    Smtp,
    /// Port 587 - mail submission (requires auth)
    Submission,
}

impl std::fmt::Display for SmtpServiceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SmtpServiceType::Smtp => write!(f, "SMTP"),
            SmtpServiceType::Submission => write!(f, "Submission"),
        }
    }
}

/// SMTP Server
pub struct SmtpServer<S: FileStorage> {
    config: SmtpConfig,
    db_pool: DatabasePool,
    file_storage: Arc<S>,
    hook_manager: Arc<HookManager>,
    queue_manager: Arc<QueueManager<S>>,
    connection_semaphore: Arc<Semaphore>,
    tls_acceptor: Option<Arc<TlsAcceptor>>,
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
            tls_acceptor: None,
        }
    }

    /// Create a new SMTP server with full config (includes TLS settings)
    pub fn with_config(
        full_config: &Config,
        db_pool: DatabasePool,
        file_storage: Arc<S>,
        hook_manager: Arc<HookManager>,
        queue_manager: Arc<QueueManager<S>>,
    ) -> Self {
        let max_connections = full_config.smtp.max_connections.unwrap_or(100);

        // Initialize TLS if configured
        let tls_acceptor = if let Some(ref tls_config) = full_config.tls {
            match create_tls_acceptor(tls_config) {
                Ok(acceptor) => {
                    info!("TLS configured successfully");
                    Some(Arc::new(acceptor))
                }
                Err(e) => {
                    warn!("Failed to initialize TLS: {}. STARTTLS will be disabled.", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            config: full_config.smtp.clone(),
            db_pool,
            file_storage,
            hook_manager,
            queue_manager,
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
            tls_acceptor,
        }
    }

    /// Run both SMTP (port 25) and Submission (port 587) servers
    pub async fn run_dual_port(self: Arc<Self>) -> Result<()> {
        let smtp_server = self.clone();
        let submission_server = self.clone();

        // Start both listeners concurrently
        let smtp_handle = tokio::spawn(async move {
            smtp_server.run_service(SmtpServiceType::Smtp).await
        });

        let submission_handle = tokio::spawn(async move {
            submission_server.run_service(SmtpServiceType::Submission).await
        });

        // Wait for both to complete (they shouldn't unless there's an error)
        tokio::select! {
            result = smtp_handle => {
                match result {
                    Ok(Ok(())) => info!("SMTP service stopped"),
                    Ok(Err(e)) => error!("SMTP service error: {}", e),
                    Err(e) => error!("SMTP task panicked: {}", e),
                }
            }
            result = submission_handle => {
                match result {
                    Ok(Ok(())) => info!("Submission service stopped"),
                    Ok(Err(e)) => error!("Submission service error: {}", e),
                    Err(e) => error!("Submission task panicked: {}", e),
                }
            }
        }

        Ok(())
    }

    /// Run a specific SMTP service (SMTP or Submission)
    pub async fn run_service(&self, service_type: SmtpServiceType) -> Result<()> {
        let (port, auth_required) = match service_type {
            SmtpServiceType::Smtp => (self.config.port, self.config.auth_required.unwrap_or(false)),
            SmtpServiceType::Submission => (self.config.submission_port, true), // Submission always requires auth
        };

        let addr = format!("{}:{}", self.config.host, port);
        let listener = TcpListener::bind(&addr).await?;

        let tls_status = if self.tls_acceptor.is_some() {
            "STARTTLS enabled"
        } else {
            "STARTTLS disabled"
        };
        info!("{} server listening on {} ({})", service_type, addr, tls_status);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    // Acquire semaphore permit
                    let permit = match self.connection_semaphore.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            warn!("{}: Max connections reached, rejecting {}", service_type, peer_addr);
                            continue;
                        }
                    };

                    // Create config for this connection
                    let mut handler_config = self.config.clone();
                    handler_config.auth_required = Some(auth_required);
                    // Enable TLS in config only if we have an acceptor
                    handler_config.tls_enabled = Some(self.tls_acceptor.is_some());

                    let handler = SmtpHandler::new(
                        handler_config,
                        self.db_pool.clone(),
                        self.file_storage.clone(),
                        self.hook_manager.clone(),
                        self.queue_manager.clone(),
                        peer_addr,
                    );

                    let service_name = service_type.to_string();
                    let tls_acceptor = self.tls_acceptor.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handler.handle_with_tls(stream, tls_acceptor).await {
                            error!("{} session error from {}: {}", service_name, peer_addr, e);
                        }
                        drop(permit);
                    });
                }
                Err(e) => {
                    error!("{}: Failed to accept connection: {}", service_type, e);
                }
            }
        }
    }

    /// Run the SMTP server on a single port (legacy method)
    pub async fn run(&self) -> Result<()> {
        self.run_service(SmtpServiceType::Smtp).await
    }
}
