//! POP3 Server
//!
//! Main POP3 server implementation for mail retrieval.

use super::command::{Pop3Command, Pop3Parser};
use super::response::Pop3Response;
use super::session::{MessageInfo, Pop3Session, SessionState};

use anyhow::{anyhow, Result};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use mairust_common::config::TlsConfig;
use mairust_storage::db::DatabasePool;
use mairust_storage::models::Message;
use mairust_storage::{FileStorage, LocalStorage};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// POP3 server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pop3Config {
    /// Listen address and port
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Enable STARTTLS
    #[serde(default)]
    pub starttls: bool,
    /// Session timeout in minutes
    #[serde(default = "default_timeout")]
    pub timeout_minutes: i64,
    /// Maximum connections
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// Server name for greeting
    #[serde(default = "default_server_name")]
    pub server_name: String,
    /// Storage path for message files
    #[serde(default = "default_storage_path")]
    pub storage_path: PathBuf,
}

fn default_storage_path() -> PathBuf {
    PathBuf::from("/var/lib/mairust/mail")
}

fn default_bind() -> String {
    "0.0.0.0:110".to_string()
}

fn default_timeout() -> i64 {
    10
}

fn default_max_connections() -> usize {
    500
}

fn default_server_name() -> String {
    "MaiRust".to_string()
}

impl Default for Pop3Config {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            starttls: false,
            timeout_minutes: default_timeout(),
            max_connections: default_max_connections(),
            server_name: default_server_name(),
            storage_path: default_storage_path(),
        }
    }
}

/// POP3 Server
pub struct Pop3Server {
    config: Pop3Config,
    db_pool: DatabasePool,
    tls_acceptor: Option<Arc<TlsAcceptor>>,
}

impl Pop3Server {
    /// Create a new POP3 server
    pub fn new(config: Pop3Config, db_pool: DatabasePool) -> Self {
        Self {
            config,
            db_pool,
            tls_acceptor: None,
        }
    }

    pub fn with_tls(config: Pop3Config, db_pool: DatabasePool, tls: Option<&TlsConfig>) -> Self {
        let tls_acceptor =
            tls.and_then(
                |tls_config| match crate::smtp::create_tls_acceptor(tls_config) {
                    Ok(acceptor) => Some(Arc::new(acceptor)),
                    Err(e) => {
                        warn!("Failed to initialize POP3 STLS acceptor: {}", e);
                        None
                    }
                },
            );

        Self {
            config,
            db_pool,
            tls_acceptor,
        }
    }

    /// Start the POP3 server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.bind).await?;
        let tls_status = if self.config.starttls && self.tls_acceptor.is_some() {
            "STLS enabled"
        } else {
            "STLS disabled"
        };
        info!(
            "POP3 server listening on {} ({})",
            self.config.bind, tls_status
        );

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let db_pool = self.db_pool.clone();
                    let config = self.config.clone();
                    let tls_acceptor = self.tls_acceptor.clone();

                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_connection(stream, addr, db_pool, config, tls_acceptor)
                                .await
                        {
                            error!("POP3 connection error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("POP3 accept error: {}", e);
                }
            }
        }
    }

    /// Handle a single POP3 connection
    async fn handle_connection(
        stream: TcpStream,
        addr: SocketAddr,
        db_pool: DatabasePool,
        config: Pop3Config,
        tls_acceptor: Option<Arc<TlsAcceptor>>,
    ) -> Result<()> {
        info!("New POP3 connection from {}", addr);

        let (reader, writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let writer = Arc::new(Mutex::new(writer));
        let session = Arc::new(Mutex::new(Pop3Session::new()));

        // Send greeting
        {
            let mut w = writer.lock().await;
            w.write_all(Pop3Response::greeting(&config.server_name).as_bytes())
                .await?;
            w.flush().await?;
        }

        let mut line = String::new();

        loop {
            line.clear();

            // Read line with timeout
            let read_result = tokio::time::timeout(
                std::time::Duration::from_secs((config.timeout_minutes * 60) as u64),
                reader.read_line(&mut line),
            )
            .await;

            match read_result {
                Ok(Ok(0)) => {
                    // Connection closed
                    info!("POP3 connection closed by client {}", addr);
                    break;
                }
                Ok(Ok(_)) => {
                    debug!("POP3 received from {}: {}", addr, line.trim());

                    // Parse and handle command
                    let cmd = Pop3Parser::parse(&line);
                    let mut upgrade_tls = false;
                    let response = match cmd {
                        Pop3Command::Stls => {
                            let sess = session.lock().await;
                            if !sess.is_authorization() {
                                Pop3Response::err("STLS only allowed in AUTHORIZATION state")
                            } else if !config.starttls || tls_acceptor.is_none() {
                                Pop3Response::err("STLS not available")
                            } else {
                                upgrade_tls = true;
                                Pop3Response::ok("Begin TLS negotiation")
                            }
                        }
                        Pop3Command::Capa => Pop3Response::capabilities_with_starttls(
                            config.starttls && tls_acceptor.is_some(),
                        ),
                        other => {
                            let (resp, should_quit) = Self::handle_command(
                                other,
                                &session,
                                &db_pool,
                                &config.storage_path,
                            )
                            .await;
                            if should_quit {
                                let mut w = writer.lock().await;
                                w.write_all(resp.as_bytes()).await?;
                                w.flush().await?;
                                break;
                            }
                            resp
                        }
                    };

                    // Send response
                    {
                        let mut w = writer.lock().await;
                        w.write_all(response.as_bytes()).await?;
                        w.flush().await?;
                    }

                    if upgrade_tls {
                        let acceptor = tls_acceptor.clone().ok_or_else(|| {
                            anyhow!("POP3 STLS requested without configured acceptor")
                        })?;
                        let read_half = reader.into_inner();
                        let writer_half = Arc::try_unwrap(writer)
                            .map_err(|_| anyhow!("Failed to unwrap POP3 writer during STLS"))?
                            .into_inner();
                        let tcp_stream = read_half.reunite(writer_half).map_err(|_| {
                            anyhow!("Failed to reunite POP3 stream halves during STLS")
                        })?;
                        let tls_stream = acceptor.accept(tcp_stream).await?;
                        info!("POP3 STLS negotiation completed for {}", addr);
                        return Self::handle_tls_connection(
                            tls_stream, addr, db_pool, config, session,
                        )
                        .await;
                    }
                }
                Ok(Err(e)) => {
                    error!("POP3 read error from {}: {}", addr, e);
                    break;
                }
                Err(_) => {
                    // Timeout
                    warn!("POP3 connection timeout for {}", addr);
                    let mut w = writer.lock().await;
                    w.write_all(Pop3Response::err("Session timeout").as_bytes())
                        .await?;
                    break;
                }
            }
        }

        info!("POP3 connection closed for {}", addr);
        Ok(())
    }

    async fn handle_tls_connection(
        tls_stream: tokio_rustls::server::TlsStream<TcpStream>,
        addr: SocketAddr,
        db_pool: DatabasePool,
        config: Pop3Config,
        session: Arc<Mutex<Pop3Session>>,
    ) -> Result<()> {
        let (reader, writer) = tokio::io::split(tls_stream);
        let mut reader = BufReader::new(reader);
        let writer = Arc::new(Mutex::new(writer));
        let mut line = String::new();

        loop {
            line.clear();
            let read_result = tokio::time::timeout(
                std::time::Duration::from_secs((config.timeout_minutes * 60) as u64),
                reader.read_line(&mut line),
            )
            .await;

            match read_result {
                Ok(Ok(0)) => break,
                Ok(Ok(_)) => {
                    let cmd = Pop3Parser::parse(&line);
                    let response = match cmd {
                        Pop3Command::Stls => Pop3Response::err("TLS already active"),
                        Pop3Command::Capa => Pop3Response::capabilities_with_starttls(false),
                        other => {
                            let (resp, should_quit) = Self::handle_command(
                                other,
                                &session,
                                &db_pool,
                                &config.storage_path,
                            )
                            .await;
                            if should_quit {
                                let mut w = writer.lock().await;
                                w.write_all(resp.as_bytes()).await?;
                                w.flush().await?;
                                break;
                            }
                            resp
                        }
                    };

                    let mut w = writer.lock().await;
                    w.write_all(response.as_bytes()).await?;
                    w.flush().await?;
                }
                Ok(Err(e)) => {
                    error!("POP3 TLS read error from {}: {}", addr, e);
                    break;
                }
                Err(_) => {
                    warn!("POP3 TLS connection timeout for {}", addr);
                    let mut w = writer.lock().await;
                    w.write_all(Pop3Response::err("Session timeout").as_bytes())
                        .await?;
                    break;
                }
            }
        }

        info!("POP3 TLS connection closed for {}", addr);
        Ok(())
    }

    /// Handle a parsed POP3 command
    async fn handle_command(
        cmd: Pop3Command,
        session: &Arc<Mutex<Pop3Session>>,
        db_pool: &DatabasePool,
        storage_path: &PathBuf,
    ) -> (String, bool) {
        match cmd {
            // Authorization state commands
            Pop3Command::User { username } => {
                let mut sess = session.lock().await;
                if !sess.is_authorization() {
                    return (Pop3Response::err("Already authenticated"), false);
                }
                sess.set_username(username);
                (Pop3Response::ok("Send password"), false)
            }

            Pop3Command::Pass { password } => Self::handle_pass(&password, session, db_pool).await,

            Pop3Command::Apop { name, digest } => {
                // APOP not implemented yet
                (Pop3Response::err("APOP not supported"), false)
            }

            // Transaction state commands
            Pop3Command::Stat => {
                let sess = session.lock().await;
                if !sess.is_transaction() {
                    return (Pop3Response::err("Not authenticated"), false);
                }
                (
                    Pop3Response::stat(sess.message_count(), sess.total_size()),
                    false,
                )
            }

            Pop3Command::List { msg } => Self::handle_list(msg, session).await,

            Pop3Command::Retr { msg } => Self::handle_retr(msg, session, storage_path).await,

            Pop3Command::Dele { msg } => {
                let mut sess = session.lock().await;
                if !sess.is_transaction() {
                    return (Pop3Response::err("Not authenticated"), false);
                }
                if sess.mark_deleted(msg) {
                    (Pop3Response::ok(&format!("Message {} deleted", msg)), false)
                } else {
                    (Pop3Response::err("No such message"), false)
                }
            }

            Pop3Command::Noop => {
                let sess = session.lock().await;
                if !sess.is_transaction() {
                    return (Pop3Response::err("Not authenticated"), false);
                }
                (Pop3Response::ok_simple(), false)
            }

            Pop3Command::Rset => {
                let mut sess = session.lock().await;
                if !sess.is_transaction() {
                    return (Pop3Response::err("Not authenticated"), false);
                }
                sess.reset_deletions();
                (
                    Pop3Response::ok(&format!("Maildrop has {} messages", sess.message_count())),
                    false,
                )
            }

            Pop3Command::Top { msg, lines } => {
                Self::handle_top(msg, lines, session, storage_path).await
            }

            Pop3Command::Uidl { msg } => Self::handle_uidl(msg, session).await,

            // Any state commands
            Pop3Command::Quit => Self::handle_quit(session, db_pool).await,

            Pop3Command::Capa => (Pop3Response::capabilities(), false),

            Pop3Command::Stls => (Pop3Response::err("Use STLS before authentication"), false),

            Pop3Command::Unknown { command } => (
                Pop3Response::err(&format!("Unknown command: {}", command)),
                false,
            ),
        }
    }

    /// Handle PASS command
    async fn handle_pass(
        password: &str,
        session: &Arc<Mutex<Pop3Session>>,
        db_pool: &DatabasePool,
    ) -> (String, bool) {
        let mut sess = session.lock().await;

        if !sess.is_authorization() {
            return (Pop3Response::err("Already authenticated"), false);
        }

        let username = match &sess.username {
            Some(u) => u.clone(),
            None => return (Pop3Response::err("USER first"), false),
        };

        drop(sess);

        let pool = db_pool.pool();

        // Query user by email
        let user: Option<(Uuid, Uuid, String, bool)> = sqlx::query_as(
            "SELECT id, tenant_id, password_hash, active FROM users WHERE email = $1",
        )
        .bind(&username)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        match user {
            Some((user_id, tenant_id, password_hash, active)) => {
                if !active {
                    return (Pop3Response::err("Account disabled"), false);
                }

                // Verify password using argon2
                let password_valid = if let Ok(parsed_hash) = PasswordHash::new(&password_hash) {
                    Argon2::default()
                        .verify_password(password.as_bytes(), &parsed_hash)
                        .is_ok()
                } else {
                    false
                };

                if !password_valid {
                    return (Pop3Response::err("Invalid password"), false);
                }

                // Get user's primary mailbox
                let mailbox: Option<(Uuid,)> = sqlx::query_as(
                    "SELECT id FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 LIMIT 1",
                )
                .bind(tenant_id)
                .bind(user_id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();

                let mailbox_id = match mailbox {
                    Some((id,)) => id,
                    None => return (Pop3Response::err("No mailbox"), false),
                };

                // Load messages
                let messages: Vec<Message> = sqlx::query_as(
                    "SELECT * FROM messages WHERE mailbox_id = $1 AND deleted = false ORDER BY received_at ASC",
                )
                .bind(mailbox_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

                let message_infos: Vec<MessageInfo> = messages
                    .iter()
                    .map(|m| MessageInfo {
                        id: m.id,
                        size: m.body_size as u64,
                        uid: m.id.to_string(),
                        body_preview: m.body_preview.clone(),
                        storage_path: m.storage_path.clone(),
                    })
                    .collect();

                let count = message_infos.len();

                let mut sess = session.lock().await;
                sess.authenticate(user_id, tenant_id, mailbox_id);
                sess.load_messages(message_infos);

                info!("POP3 user {} authenticated, {} messages", username, count);

                (
                    Pop3Response::ok(&format!("Maildrop has {} messages", count)),
                    false,
                )
            }
            None => (Pop3Response::err("Invalid user"), false),
        }
    }

    /// Handle LIST command
    async fn handle_list(msg: Option<u32>, session: &Arc<Mutex<Pop3Session>>) -> (String, bool) {
        let sess = session.lock().await;

        if !sess.is_transaction() {
            return (Pop3Response::err("Not authenticated"), false);
        }

        match msg {
            Some(num) => {
                // Single message listing
                if let Some(message) = sess.get_message(num) {
                    (Pop3Response::list_single(num, message.size), false)
                } else {
                    (Pop3Response::err("No such message"), false)
                }
            }
            None => {
                // List all messages
                let mut response =
                    Pop3Response::list_header(sess.message_count(), sess.total_size());
                for (num, size) in sess.list_messages() {
                    response.push_str(&Pop3Response::list_line(num, size));
                }
                response.push_str(&Pop3Response::terminator());
                (response, false)
            }
        }
    }

    /// Handle RETR command
    async fn handle_retr(
        msg: u32,
        session: &Arc<Mutex<Pop3Session>>,
        storage_path: &PathBuf,
    ) -> (String, bool) {
        let sess = session.lock().await;

        if !sess.is_transaction() {
            return (Pop3Response::err("Not authenticated"), false);
        }

        let message_info = match sess.get_message(msg) {
            Some(m) => m.clone(),
            None => return (Pop3Response::err("No such message"), false),
        };

        drop(sess);

        // Initialize file storage and read the full message
        let storage = match LocalStorage::from_path(storage_path) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to initialize storage for RETR: {}", e);
                return (Pop3Response::err("Failed to read message"), false);
            }
        };

        // Read full message from storage
        let message_data = match storage.read(&message_info.storage_path).await {
            Ok(data) => data,
            Err(e) => {
                // Fall back to body_preview if storage read fails
                warn!(
                    "Failed to read message from storage: {}, falling back to preview",
                    e
                );
                let body = message_info.body_preview.unwrap_or_default();
                // Build body content first to calculate accurate size
                let mut body_content = String::new();
                for line in body.lines() {
                    body_content.push_str(&Pop3Response::byte_stuff_line(line));
                    body_content.push_str("\r\n");
                }
                let mut response = Pop3Response::retr_header(body_content.len() as u64);
                response.push_str(&body_content);
                response.push_str(&Pop3Response::terminator());
                return (response, false);
            }
        };

        // Convert to UTF-8 and build body content first to get accurate size
        // This ensures the octet count matches the actual payload sent
        let body = String::from_utf8_lossy(&message_data);
        let mut body_content = String::new();

        // Add body with byte-stuffing for POP3 protocol compliance
        for line in body.lines() {
            body_content.push_str(&Pop3Response::byte_stuff_line(line));
            body_content.push_str("\r\n");
        }

        // Now build response with accurate size
        let mut response = Pop3Response::retr_header(body_content.len() as u64);
        response.push_str(&body_content);
        response.push_str(&Pop3Response::terminator());
        (response, false)
    }

    /// Handle TOP command
    async fn handle_top(
        msg: u32,
        lines: u32,
        session: &Arc<Mutex<Pop3Session>>,
        storage_path: &PathBuf,
    ) -> (String, bool) {
        let sess = session.lock().await;

        if !sess.is_transaction() {
            return (Pop3Response::err("Not authenticated"), false);
        }

        let message_info = match sess.get_message(msg) {
            Some(m) => m.clone(),
            None => return (Pop3Response::err("No such message"), false),
        };

        drop(sess);

        // Initialize file storage and read the full message
        let storage = match LocalStorage::from_path(storage_path) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to initialize storage for TOP: {}", e);
                return (Pop3Response::err("Failed to read message"), false);
            }
        };

        // Read full message from storage
        let message_data = match storage.read(&message_info.storage_path).await {
            Ok(data) => String::from_utf8_lossy(&data).to_string(),
            Err(e) => {
                // Fall back to body_preview if storage read fails
                warn!("Failed to read message from storage for TOP: {}", e);
                message_info.body_preview.unwrap_or_default()
            }
        };

        let mut response = Pop3Response::top_header();

        // Split message into headers and body
        let mut in_body = false;
        let mut body_lines_count = 0;

        for line in message_data.lines() {
            if !in_body {
                // Output headers
                response.push_str(&Pop3Response::byte_stuff_line(line));
                response.push_str("\r\n");
                // Empty line indicates end of headers
                if line.is_empty() {
                    in_body = true;
                }
            } else {
                // Output body lines up to the requested limit
                if body_lines_count >= lines as usize {
                    break;
                }
                response.push_str(&Pop3Response::byte_stuff_line(line));
                response.push_str("\r\n");
                body_lines_count += 1;
            }
        }

        response.push_str(&Pop3Response::terminator());
        (response, false)
    }

    /// Handle UIDL command
    async fn handle_uidl(msg: Option<u32>, session: &Arc<Mutex<Pop3Session>>) -> (String, bool) {
        let sess = session.lock().await;

        if !sess.is_transaction() {
            return (Pop3Response::err("Not authenticated"), false);
        }

        match msg {
            Some(num) => {
                // Single message UID
                if let Some(message) = sess.get_message(num) {
                    (Pop3Response::uidl_single(num, &message.uid), false)
                } else {
                    (Pop3Response::err("No such message"), false)
                }
            }
            None => {
                // List all UIDs
                let mut response = Pop3Response::uidl_header();
                for (num, uid) in sess.uidl_messages() {
                    response.push_str(&Pop3Response::uidl_line(num, &uid));
                }
                response.push_str(&Pop3Response::terminator());
                (response, false)
            }
        }
    }

    /// Handle QUIT command
    async fn handle_quit(
        session: &Arc<Mutex<Pop3Session>>,
        db_pool: &DatabasePool,
    ) -> (String, bool) {
        let mut sess = session.lock().await;

        if sess.is_transaction() {
            sess.enter_update();

            // Delete marked messages
            let deleted_ids = sess.get_deleted_messages();
            let pool = db_pool.pool();

            for id in deleted_ids {
                let _ = sqlx::query("DELETE FROM messages WHERE id = $1")
                    .bind(id)
                    .execute(pool)
                    .await;
            }
        }

        (Pop3Response::ok("Goodbye"), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Pop3Config::default();
        assert_eq!(config.bind, "0.0.0.0:110");
        assert!(!config.starttls);
        assert_eq!(config.timeout_minutes, 10);
    }
}
