//! IMAP Server
//!
//! Full-featured IMAP server implementation with read/write mail access.

use super::command::{FetchItem, ImapCommand, SearchCriteria, SequenceSet, StoreFlags, StoreOperation, TaggedCommand};
use super::parser::ImapParser;
use super::response::ImapResponse;
use super::session::{ImapSession, SelectedMailbox, SessionState};

use anyhow::Result;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
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
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// IMAP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
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
    /// Storage path for message files
    #[serde(default = "default_storage_path")]
    pub storage_path: PathBuf,
}

fn default_storage_path() -> PathBuf {
    PathBuf::from("/var/lib/mairust/mail")
}

fn default_bind() -> String {
    "0.0.0.0:143".to_string()
}

fn default_timeout() -> i64 {
    30
}

fn default_max_connections() -> usize {
    1000
}

impl Default for ImapConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            starttls: false,
            timeout_minutes: default_timeout(),
            max_connections: default_max_connections(),
            storage_path: default_storage_path(),
        }
    }
}

/// IMAP Server
pub struct ImapServer {
    config: ImapConfig,
    db_pool: DatabasePool,
}

impl ImapServer {
    /// Create a new IMAP server
    pub fn new(config: ImapConfig, db_pool: DatabasePool) -> Self {
        Self { config, db_pool }
    }

    /// Start the IMAP server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.bind).await?;
        info!("IMAP server listening on {}", self.config.bind);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let db_pool = self.db_pool.clone();
                    let config = self.config.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, addr, db_pool, config).await
                        {
                            error!("Connection error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Handle a single IMAP connection
    async fn handle_connection(
        stream: TcpStream,
        addr: SocketAddr,
        db_pool: DatabasePool,
        config: ImapConfig,
    ) -> Result<()> {
        info!("New IMAP connection from {}", addr);

        let (reader, writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let writer = Arc::new(Mutex::new(writer));
        let session = Arc::new(Mutex::new(ImapSession::new()));

        // Send greeting
        {
            let mut w = writer.lock().await;
            w.write_all(ImapResponse::greeting().as_bytes()).await?;
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
                    info!("Connection closed by client {}", addr);
                    break;
                }
                Ok(Ok(_)) => {
                    debug!("Received from {}: {}", addr, line.trim());

                    // Parse command
                    let response = match ImapParser::parse(&line) {
                        Some(cmd) => {
                            Self::handle_command(cmd, &session, &db_pool, &config.storage_path).await
                        }
                        None => {
                            "* BAD Invalid command\r\n".to_string()
                        }
                    };

                    // Send response
                    {
                        let mut w = writer.lock().await;
                        w.write_all(response.as_bytes()).await?;
                        w.flush().await?;
                    }

                    // Check if we should close
                    {
                        let sess = session.lock().await;
                        if sess.state == SessionState::Logout {
                            break;
                        }
                    }
                }
                Ok(Err(e)) => {
                    error!("Read error from {}: {}", addr, e);
                    break;
                }
                Err(_) => {
                    // Timeout
                    warn!("Connection timeout for {}", addr);
                    let mut w = writer.lock().await;
                    w.write_all(ImapResponse::bye("Connection timeout").as_bytes())
                        .await?;
                    break;
                }
            }
        }

        info!("IMAP connection closed for {}", addr);
        Ok(())
    }

    /// Handle a parsed IMAP command
    async fn handle_command(
        cmd: TaggedCommand,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
        storage_path: &PathBuf,
    ) -> String {
        let tag = &cmd.tag;

        match cmd.command {
            // Any state commands
            ImapCommand::Capability => {
                format!("{}{}", ImapResponse::capability(), ImapResponse::ok(tag, "CAPABILITY completed"))
            }
            ImapCommand::Noop => {
                session.lock().await.update_activity();
                ImapResponse::ok(tag, "NOOP completed")
            }
            ImapCommand::Logout => {
                session.lock().await.logout();
                format!(
                    "{}{}",
                    ImapResponse::bye("Logging out"),
                    ImapResponse::ok(tag, "LOGOUT completed")
                )
            }

            // Authentication
            ImapCommand::Login { username, password } => {
                Self::handle_login(tag, &username, &password, session, db_pool).await
            }
            ImapCommand::Authenticate { mechanism, initial_response } => {
                // For now, only support PLAIN
                if mechanism != "PLAIN" {
                    return ImapResponse::no(tag, "Unsupported authentication mechanism");
                }
                // TODO: Handle PLAIN authentication
                ImapResponse::no(tag, "AUTHENTICATE not yet implemented")
            }

            // Authenticated state commands
            ImapCommand::Select { mailbox } => {
                Self::handle_select(tag, &mailbox, false, session, db_pool).await
            }
            ImapCommand::Examine { mailbox } => {
                Self::handle_select(tag, &mailbox, true, session, db_pool).await
            }
            ImapCommand::List { reference, pattern } => {
                Self::handle_list(tag, &reference, &pattern, session, db_pool).await
            }
            ImapCommand::Lsub { reference, pattern } => {
                // LSUB returns subscribed mailboxes - for now, same as LIST
                Self::handle_list(tag, &reference, &pattern, session, db_pool).await
            }
            ImapCommand::Status { mailbox, items } => {
                Self::handle_status(tag, &mailbox, &items, session, db_pool).await
            }
            ImapCommand::Close => {
                let mut sess = session.lock().await;
                if sess.is_selected() {
                    sess.close_mailbox();
                    ImapResponse::ok(tag, "CLOSE completed")
                } else {
                    ImapResponse::no(tag, "No mailbox selected")
                }
            }

            // Selected state commands
            ImapCommand::Check => {
                let sess = session.lock().await;
                if sess.is_selected() {
                    ImapResponse::ok(tag, "CHECK completed")
                } else {
                    ImapResponse::no(tag, "No mailbox selected")
                }
            }
            ImapCommand::Fetch { sequence, items, uid } => {
                Self::handle_fetch(tag, &sequence, &items, uid, session, db_pool, storage_path).await
            }
            ImapCommand::Search { criteria, uid } => {
                Self::handle_search(tag, &criteria, uid, session, db_pool).await
            }

            // Write operations - Mailbox management
            ImapCommand::Create { mailbox } => {
                Self::handle_create(tag, &mailbox, session, db_pool).await
            }
            ImapCommand::Delete { mailbox } => {
                Self::handle_delete(tag, &mailbox, session, db_pool).await
            }
            ImapCommand::Rename { old_mailbox, new_mailbox } => {
                Self::handle_rename(tag, &old_mailbox, &new_mailbox, session, db_pool).await
            }
            ImapCommand::Subscribe { mailbox } => {
                Self::handle_subscribe(tag, &mailbox, true, session, db_pool).await
            }
            ImapCommand::Unsubscribe { mailbox } => {
                Self::handle_subscribe(tag, &mailbox, false, session, db_pool).await
            }

            // Write operations - Message operations
            ImapCommand::Store { sequence, flags, uid } => {
                Self::handle_store(tag, &sequence, &flags, uid, session, db_pool).await
            }
            ImapCommand::Copy { sequence, mailbox, uid } => {
                Self::handle_copy(tag, &sequence, &mailbox, uid, session, db_pool).await
            }
            ImapCommand::Move { sequence, mailbox, uid } => {
                Self::handle_move(tag, &sequence, &mailbox, uid, session, db_pool).await
            }
            ImapCommand::Expunge => {
                Self::handle_expunge(tag, session, db_pool).await
            }
            ImapCommand::Append { mailbox, flags, date, message } => {
                Self::handle_append(tag, &mailbox, &flags, date.as_deref(), &message, session, db_pool, storage_path).await
            }

            // Extensions
            ImapCommand::Idle => {
                // IDLE mode - client wants to wait for updates
                // For now, just acknowledge and wait for DONE
                ImapResponse::continue_req()
            }
            ImapCommand::Done => {
                ImapResponse::ok(tag, "IDLE terminated")
            }
            ImapCommand::Namespace => {
                format!(
                    "{}{}",
                    ImapResponse::namespace(),
                    ImapResponse::ok(tag, "NAMESPACE completed")
                )
            }

            ImapCommand::Unknown { command } => {
                ImapResponse::bad(tag, &format!("Unknown command: {}", command))
            }
        }
    }

    /// Handle LOGIN command
    async fn handle_login(
        tag: &str,
        username: &str,
        password: &str,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let pool = db_pool.pool();

        // Query user by email
        let user: Option<(Uuid, Uuid, String, String, bool)> = sqlx::query_as(
            "SELECT id, tenant_id, email, password_hash, active FROM users WHERE email = $1",
        )
        .bind(username)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        match user {
            Some((user_id, tenant_id, email, password_hash, active)) => {
                if !active {
                    return ImapResponse::no(tag, "Account is disabled");
                }

                // Verify password using argon2
                let password_valid = if let Ok(parsed_hash) = PasswordHash::new(&password_hash) {
                    Argon2::default()
                        .verify_password(password.as_bytes(), &parsed_hash)
                        .is_ok()
                } else {
                    false
                };

                if password_valid {
                    let mut sess = session.lock().await;
                    sess.authenticate(user_id, tenant_id, email);
                    ImapResponse::ok(tag, "LOGIN completed")
                } else {
                    ImapResponse::no(tag, "Invalid credentials")
                }
            }
            None => ImapResponse::no(tag, "Invalid credentials"),
        }
    }

    /// Handle SELECT/EXAMINE command
    async fn handle_select(
        tag: &str,
        mailbox_name: &str,
        readonly: bool,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_authenticated() {
            return ImapResponse::no(tag, "Not authenticated");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };

        let user_id = match sess.user_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No user context"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Find mailbox by address (INBOX is special)
        let mailbox_query = if mailbox_name.to_uppercase() == "INBOX" {
            // Get primary mailbox for user
            sqlx::query_as::<_, (Uuid, String)>(
                "SELECT id, address FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 LIMIT 1",
            )
            .bind(tenant_id)
            .bind(user_id)
        } else {
            // SECURITY: Must filter by user_id to prevent cross-user mailbox access
            sqlx::query_as::<_, (Uuid, String)>(
                "SELECT id, address FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND address = $3",
            )
            .bind(tenant_id)
            .bind(user_id)
            .bind(mailbox_name)
        };

        let mailbox_result = mailbox_query.fetch_optional(pool).await;

        match mailbox_result {
            Ok(Some((mailbox_id, mailbox_address))) => {
                // Get messages for this mailbox
                let messages: Vec<Message> = sqlx::query_as(
                    "SELECT * FROM messages WHERE mailbox_id = $1 ORDER BY received_at ASC",
                )
                .bind(mailbox_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

                let mut selected = SelectedMailbox::new(mailbox_id, mailbox_address.clone());
                selected.update_with_messages(&messages);

                let mut response = String::new();

                // Send mailbox information
                response.push_str(&ImapResponse::mailbox_flags(&["\\Answered", "\\Flagged", "\\Deleted", "\\Seen", "\\Draft"]));
                response.push_str(&ImapResponse::permanent_flags(&["\\Answered", "\\Flagged", "\\Deleted", "\\Seen", "\\Draft", "\\*"]));
                response.push_str(&ImapResponse::exists(selected.exists));
                response.push_str(&ImapResponse::recent(selected.recent));

                if let Some(unseen) = selected.first_unseen {
                    response.push_str(&ImapResponse::unseen(unseen));
                }

                response.push_str(&ImapResponse::uid_validity(selected.uid_validity));
                response.push_str(&ImapResponse::uid_next(selected.uid_next));

                // Update session
                let mut sess = session.lock().await;
                sess.select(selected, readonly);

                let mode = if readonly { "[READ-ONLY]" } else { "[READ-WRITE]" };
                response.push_str(&ImapResponse::ok(tag, &format!("{} SELECT completed", mode)));

                response
            }
            Ok(None) => ImapResponse::no(tag, "Mailbox not found"),
            Err(e) => {
                error!("Database error in SELECT: {}", e);
                ImapResponse::no(tag, "Internal server error")
            }
        }
    }

    /// Handle LIST command
    async fn handle_list(
        tag: &str,
        _reference: &str,
        pattern: &str,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_authenticated() {
            return ImapResponse::no(tag, "Not authenticated");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };
        let user_id = sess.user_id;
        drop(sess);

        let pool = db_pool.pool();

        // Get mailboxes for this tenant/user
        let mailboxes: Vec<(String,)> = if let Some(uid) = user_id {
            sqlx::query_as("SELECT address FROM mailboxes WHERE tenant_id = $1 AND user_id = $2")
                .bind(tenant_id)
                .bind(uid)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        } else {
            sqlx::query_as("SELECT address FROM mailboxes WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        };

        let mut response = String::new();

        // Always include INBOX
        response.push_str(&ImapResponse::list(&["\\HasNoChildren"], "/", "INBOX"));

        // Filter mailboxes by pattern
        for (address,) in mailboxes {
            if pattern == "*" || pattern == "%" || address.contains(pattern.trim_matches('*').trim_matches('%')) {
                response.push_str(&ImapResponse::list(&["\\HasNoChildren"], "/", &address));
            }
        }

        response.push_str(&ImapResponse::ok(tag, "LIST completed"));
        response
    }

    /// Handle STATUS command
    async fn handle_status(
        tag: &str,
        mailbox_name: &str,
        items: &[String],
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_authenticated() {
            return ImapResponse::no(tag, "Not authenticated");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };

        let user_id = match sess.user_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No user context"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Get mailbox
        // SECURITY: Must filter by user_id to prevent cross-user mailbox access
        let mailbox: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND address = $3",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(mailbox_name)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        match mailbox {
            Some((mailbox_id,)) => {
                // Get message counts
                let total: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM messages WHERE mailbox_id = $1",
                )
                .bind(mailbox_id)
                .fetch_one(pool)
                .await
                .unwrap_or((0,));

                let unseen: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM messages WHERE mailbox_id = $1 AND seen = false",
                )
                .bind(mailbox_id)
                .fetch_one(pool)
                .await
                .unwrap_or((0,));

                let mut status_items = Vec::new();
                for item in items {
                    match item.to_uppercase().as_str() {
                        "MESSAGES" => status_items.push(("MESSAGES".to_string(), total.0 as u32)),
                        "UNSEEN" => status_items.push(("UNSEEN".to_string(), unseen.0 as u32)),
                        "RECENT" => status_items.push(("RECENT".to_string(), 0)),
                        "UIDNEXT" => status_items.push(("UIDNEXT".to_string(), (total.0 + 1) as u32)),
                        "UIDVALIDITY" => status_items.push(("UIDVALIDITY".to_string(), 1)),
                        _ => {}
                    }
                }

                format!(
                    "{}{}",
                    ImapResponse::status(mailbox_name, &status_items),
                    ImapResponse::ok(tag, "STATUS completed")
                )
            }
            None => ImapResponse::no(tag, "Mailbox not found"),
        }
    }

    /// Handle FETCH command
    async fn handle_fetch(
        tag: &str,
        sequence: &SequenceSet,
        items: &[FetchItem],
        uid_mode: bool,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
        storage_path: &PathBuf,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_selected() {
            return ImapResponse::no(tag, "No mailbox selected");
        }

        let selected = match &sess.selected_mailbox {
            Some(s) => s.clone(),
            None => return ImapResponse::no(tag, "No mailbox selected"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Get messages for the sequence set
        let messages: Vec<Message> = sqlx::query_as(
            "SELECT * FROM messages WHERE mailbox_id = $1 ORDER BY received_at ASC",
        )
        .bind(selected.id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        // Initialize storage for reading full message bodies
        let storage = match LocalStorage::from_path(storage_path) {
            Ok(s) => Some(s),
            Err(e) => {
                warn!("Failed to initialize storage for FETCH: {}", e);
                None
            }
        };

        let mut response = String::new();
        let max_seq = messages.len() as u32;

        for (idx, msg) in messages.iter().enumerate() {
            let seq = (idx + 1) as u32;
            let msg_uid = Self::message_id_to_uid(&msg.id);

            // Check if this message is in the sequence set
            let in_set = if uid_mode {
                sequence.contains(msg_uid, msg_uid)
            } else {
                sequence.contains(seq, max_seq)
            };

            if !in_set {
                continue;
            }

            // Build FETCH response items
            let mut fetch_items: Vec<(String, String)> = Vec::new();

            for item in items {
                match item {
                    FetchItem::Flags => {
                        let flags = ImapResponse::format_flags(
                            msg.seen,
                            msg.answered,
                            msg.flagged,
                            msg.deleted,
                            msg.draft,
                        );
                        fetch_items.push(("FLAGS".to_string(), flags));
                    }
                    FetchItem::Uid => {
                        fetch_items.push(("UID".to_string(), msg_uid.to_string()));
                    }
                    FetchItem::InternalDate => {
                        let date = ImapResponse::format_internal_date(&msg.received_at);
                        fetch_items.push(("INTERNALDATE".to_string(), date));
                    }
                    FetchItem::Rfc822Size => {
                        fetch_items.push(("RFC822.SIZE".to_string(), msg.body_size.to_string()));
                    }
                    FetchItem::Envelope => {
                        let headers = &msg.headers;
                        let envelope = ImapResponse::format_envelope(
                            headers.get("date").and_then(|v| v.as_str()),
                            msg.subject.as_deref(),
                            msg.from_address.as_deref(),
                            headers.get("to").and_then(|v| v.as_str()),
                            headers.get("cc").and_then(|v| v.as_str()),
                            msg.message_id_header.as_deref(),
                        );
                        fetch_items.push(("ENVELOPE".to_string(), envelope));
                    }
                    FetchItem::BodyStructure | FetchItem::Body => {
                        let lines = msg.body_preview.as_ref().map(|p| p.lines().count() as u32).unwrap_or(0);
                        let structure = ImapResponse::format_body_structure_simple(msg.body_size as u64, lines);
                        fetch_items.push(("BODYSTRUCTURE".to_string(), structure));
                    }
                    FetchItem::All => {
                        // FLAGS, INTERNALDATE, RFC822.SIZE, ENVELOPE
                        let flags = ImapResponse::format_flags(
                            msg.seen, msg.answered, msg.flagged, msg.deleted, msg.draft,
                        );
                        fetch_items.push(("FLAGS".to_string(), flags));
                        fetch_items.push(("INTERNALDATE".to_string(),
                            ImapResponse::format_internal_date(&msg.received_at)));
                        fetch_items.push(("RFC822.SIZE".to_string(), msg.body_size.to_string()));
                    }
                    FetchItem::Fast => {
                        // FLAGS, INTERNALDATE, RFC822.SIZE
                        let flags = ImapResponse::format_flags(
                            msg.seen, msg.answered, msg.flagged, msg.deleted, msg.draft,
                        );
                        fetch_items.push(("FLAGS".to_string(), flags));
                        fetch_items.push(("INTERNALDATE".to_string(),
                            ImapResponse::format_internal_date(&msg.received_at)));
                        fetch_items.push(("RFC822.SIZE".to_string(), msg.body_size.to_string()));
                    }
                    FetchItem::Full => {
                        // FLAGS, INTERNALDATE, RFC822.SIZE, ENVELOPE, BODY
                        let flags = ImapResponse::format_flags(
                            msg.seen, msg.answered, msg.flagged, msg.deleted, msg.draft,
                        );
                        fetch_items.push(("FLAGS".to_string(), flags));
                        fetch_items.push(("INTERNALDATE".to_string(),
                            ImapResponse::format_internal_date(&msg.received_at)));
                        fetch_items.push(("RFC822.SIZE".to_string(), msg.body_size.to_string()));
                        let lines = msg.body_preview.as_ref().map(|p| p.lines().count() as u32).unwrap_or(0);
                        fetch_items.push(("BODYSTRUCTURE".to_string(),
                            ImapResponse::format_body_structure_simple(msg.body_size as u64, lines)));
                    }
                    FetchItem::BodySection { section, .. } | FetchItem::BodyPeek { section, .. } => {
                        // Read full message body from storage
                        let body_key = format!("BODY[{}]", section);
                        if let Some(ref storage) = storage {
                            match storage.read(&msg.storage_path).await {
                                Ok(data) => {
                                    let body = String::from_utf8_lossy(&data);
                                    fetch_items.push((body_key, format!("{{{}}}\r\n{}", data.len(), body)));
                                }
                                Err(e) => {
                                    // Fall back to body_preview if storage read fails
                                    warn!("Failed to read message from storage: {}", e);
                                    if let Some(preview) = &msg.body_preview {
                                        fetch_items.push((body_key, format!("{{{}}}\r\n{}", preview.len(), preview)));
                                    }
                                }
                            }
                        } else if let Some(preview) = &msg.body_preview {
                            // No storage available, use preview
                            fetch_items.push((body_key, format!("{{{}}}\r\n{}", preview.len(), preview)));
                        }
                    }
                }
            }

            response.push_str(&ImapResponse::fetch(seq, &fetch_items));
        }

        response.push_str(&ImapResponse::ok(tag, "FETCH completed"));
        response
    }

    /// Handle SEARCH command
    async fn handle_search(
        tag: &str,
        criteria: &SearchCriteria,
        uid_mode: bool,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_selected() {
            return ImapResponse::no(tag, "No mailbox selected");
        }

        let selected = match &sess.selected_mailbox {
            Some(s) => s.clone(),
            None => return ImapResponse::no(tag, "No mailbox selected"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Get messages
        let messages: Vec<Message> = sqlx::query_as(
            "SELECT * FROM messages WHERE mailbox_id = $1 ORDER BY received_at ASC",
        )
        .bind(selected.id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut results = Vec::new();

        for (idx, msg) in messages.iter().enumerate() {
            let seq = (idx + 1) as u32;
            let msg_uid = Self::message_id_to_uid(&msg.id);

            if Self::matches_criteria(msg, criteria) {
                if uid_mode {
                    results.push(msg_uid);
                } else {
                    results.push(seq);
                }
            }
        }

        format!(
            "{}{}",
            ImapResponse::search(&results),
            ImapResponse::ok(tag, "SEARCH completed")
        )
    }

    /// Check if a message matches search criteria
    fn matches_criteria(msg: &Message, criteria: &SearchCriteria) -> bool {
        match criteria {
            SearchCriteria::All => true,
            SearchCriteria::Answered => msg.answered,
            SearchCriteria::Deleted => msg.deleted,
            SearchCriteria::Draft => msg.draft,
            SearchCriteria::Flagged => msg.flagged,
            SearchCriteria::Seen => msg.seen,
            SearchCriteria::Unanswered => !msg.answered,
            SearchCriteria::Undeleted => !msg.deleted,
            SearchCriteria::Undraft => !msg.draft,
            SearchCriteria::Unflagged => !msg.flagged,
            SearchCriteria::Unseen => !msg.seen,
            SearchCriteria::New => !msg.seen,
            SearchCriteria::Old => msg.seen,
            SearchCriteria::Recent => {
                let age = chrono::Utc::now() - msg.created_at;
                age.num_hours() < 24
            }
            SearchCriteria::From(s) => {
                msg.from_address
                    .as_ref()
                    .map(|f| f.to_lowercase().contains(&s.to_lowercase()))
                    .unwrap_or(false)
            }
            SearchCriteria::To(s) => {
                if let Some(to) = msg.to_addresses.as_array() {
                    to.iter().any(|addr| {
                        addr.as_str()
                            .map(|a| a.to_lowercase().contains(&s.to_lowercase()))
                            .unwrap_or(false)
                    })
                } else {
                    false
                }
            }
            SearchCriteria::Subject(s) => {
                msg.subject
                    .as_ref()
                    .map(|subj| subj.to_lowercase().contains(&s.to_lowercase()))
                    .unwrap_or(false)
            }
            SearchCriteria::Body(s) | SearchCriteria::Text(s) => {
                msg.body_preview
                    .as_ref()
                    .map(|body| body.to_lowercase().contains(&s.to_lowercase()))
                    .unwrap_or(false)
            }
            SearchCriteria::Larger(size) => msg.body_size > (*size as i64),
            SearchCriteria::Smaller(size) => msg.body_size < (*size as i64),
            SearchCriteria::Not(inner) => !Self::matches_criteria(msg, inner),
            SearchCriteria::And(criteria_list) => {
                criteria_list.iter().all(|c| Self::matches_criteria(msg, c))
            }
            SearchCriteria::Or(a, b) => {
                Self::matches_criteria(msg, a) || Self::matches_criteria(msg, b)
            }
            _ => true, // Default to matching for unimplemented criteria
        }
    }

    /// Convert message ID to UID
    fn message_id_to_uid(id: &Uuid) -> u32 {
        let bytes = id.as_bytes();
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    // ========================================================================
    // Write Operations - Mailbox Management
    // ========================================================================

    /// Handle CREATE command
    async fn handle_create(
        tag: &str,
        mailbox_name: &str,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_authenticated() {
            return ImapResponse::no(tag, "Not authenticated");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };
        let user_id = match sess.user_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No user context"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Check if mailbox already exists
        let exists: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM mailboxes WHERE tenant_id = $1 AND address = $2",
        )
        .bind(tenant_id)
        .bind(mailbox_name)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        if exists.is_some() {
            return ImapResponse::no(tag, "Mailbox already exists");
        }

        // Get user's domain
        let domain: Option<(Uuid,)> = sqlx::query_as(
            "SELECT d.id FROM domains d
             JOIN users u ON u.tenant_id = d.tenant_id
             WHERE u.id = $1 LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        let domain_id = match domain {
            Some((id,)) => id,
            None => return ImapResponse::no(tag, "No domain found"),
        };

        // Create the mailbox
        let mailbox_id = Uuid::new_v4();
        let result = sqlx::query(
            "INSERT INTO mailboxes (id, tenant_id, domain_id, user_id, address, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
        )
        .bind(mailbox_id)
        .bind(tenant_id)
        .bind(domain_id)
        .bind(user_id)
        .bind(mailbox_name)
        .execute(pool)
        .await;

        match result {
            Ok(_) => {
                info!("Created mailbox {} for user {}", mailbox_name, user_id);
                ImapResponse::ok(tag, "CREATE completed")
            }
            Err(e) => {
                error!("Failed to create mailbox: {}", e);
                ImapResponse::no(tag, "Failed to create mailbox")
            }
        }
    }

    /// Handle DELETE command
    async fn handle_delete(
        tag: &str,
        mailbox_name: &str,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_authenticated() {
            return ImapResponse::no(tag, "Not authenticated");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };
        let user_id = match sess.user_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No user context"),
        };
        drop(sess);

        // Cannot delete INBOX
        if mailbox_name.to_uppercase() == "INBOX" {
            return ImapResponse::no(tag, "Cannot delete INBOX");
        }

        let pool = db_pool.pool();

        // Find and delete the mailbox
        let result = sqlx::query(
            "DELETE FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND address = $3",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(mailbox_name)
        .execute(pool)
        .await;

        match result {
            Ok(r) if r.rows_affected() > 0 => {
                info!("Deleted mailbox {} for user {}", mailbox_name, user_id);
                ImapResponse::ok(tag, "DELETE completed")
            }
            Ok(_) => ImapResponse::no(tag, "Mailbox not found"),
            Err(e) => {
                error!("Failed to delete mailbox: {}", e);
                ImapResponse::no(tag, "Failed to delete mailbox")
            }
        }
    }

    /// Handle RENAME command
    async fn handle_rename(
        tag: &str,
        old_name: &str,
        new_name: &str,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_authenticated() {
            return ImapResponse::no(tag, "Not authenticated");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };
        let user_id = match sess.user_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No user context"),
        };
        drop(sess);

        // Cannot rename INBOX
        if old_name.to_uppercase() == "INBOX" {
            return ImapResponse::no(tag, "Cannot rename INBOX");
        }

        let pool = db_pool.pool();

        // Rename the mailbox
        let result = sqlx::query(
            "UPDATE mailboxes SET address = $4, updated_at = NOW()
             WHERE tenant_id = $1 AND user_id = $2 AND address = $3",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(old_name)
        .bind(new_name)
        .execute(pool)
        .await;

        match result {
            Ok(r) if r.rows_affected() > 0 => {
                info!("Renamed mailbox {} to {} for user {}", old_name, new_name, user_id);
                ImapResponse::ok(tag, "RENAME completed")
            }
            Ok(_) => ImapResponse::no(tag, "Mailbox not found"),
            Err(e) => {
                error!("Failed to rename mailbox: {}", e);
                ImapResponse::no(tag, "Failed to rename mailbox")
            }
        }
    }

    /// Handle SUBSCRIBE/UNSUBSCRIBE command
    async fn handle_subscribe(
        tag: &str,
        mailbox_name: &str,
        subscribe: bool,
        session: &Arc<Mutex<ImapSession>>,
        _db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_authenticated() {
            return ImapResponse::no(tag, "Not authenticated");
        }
        drop(sess);

        // For now, we don't track subscriptions separately - all mailboxes are subscribed
        // This could be implemented with a mailbox_subscriptions table
        let action = if subscribe { "SUBSCRIBE" } else { "UNSUBSCRIBE" };
        debug!("{} to mailbox {}", action, mailbox_name);

        ImapResponse::ok(tag, &format!("{} completed", action))
    }

    // ========================================================================
    // Write Operations - Message Operations
    // ========================================================================

    /// Handle STORE command
    async fn handle_store(
        tag: &str,
        sequence: &SequenceSet,
        flags: &StoreFlags,
        uid_mode: bool,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_selected() {
            return ImapResponse::no(tag, "No mailbox selected");
        }

        // Check if mailbox is read-only
        if sess.is_readonly() {
            return ImapResponse::no(tag, "Mailbox is read-only");
        }

        let selected = match &sess.selected_mailbox {
            Some(s) => s.clone(),
            None => return ImapResponse::no(tag, "No mailbox selected"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Get messages for the sequence set
        let messages: Vec<Message> = sqlx::query_as(
            "SELECT * FROM messages WHERE mailbox_id = $1 ORDER BY received_at ASC",
        )
        .bind(selected.id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut response = String::new();
        let max_seq = messages.len() as u32;

        for (idx, msg) in messages.iter().enumerate() {
            let seq = (idx + 1) as u32;
            let msg_uid = Self::message_id_to_uid(&msg.id);

            // Check if this message is in the sequence set
            let in_set = if uid_mode {
                sequence.contains(msg_uid, msg_uid)
            } else {
                sequence.contains(seq, max_seq)
            };

            if !in_set {
                continue;
            }

            // Parse and apply flag changes
            let (new_seen, new_answered, new_flagged, new_deleted, new_draft) =
                Self::apply_flag_changes(msg, flags);

            // Update the message in database
            let update_result = sqlx::query(
                "UPDATE messages SET seen = $2, answered = $3, flagged = $4, deleted = $5, draft = $6
                 WHERE id = $1",
            )
            .bind(msg.id)
            .bind(new_seen)
            .bind(new_answered)
            .bind(new_flagged)
            .bind(new_deleted)
            .bind(new_draft)
            .execute(pool)
            .await;

            if let Err(e) = update_result {
                error!("Failed to update message flags: {}", e);
                continue;
            }

            // If not silent, send FETCH response with new flags
            if !flags.silent {
                let flags_str = ImapResponse::format_flags(
                    new_seen, new_answered, new_flagged, new_deleted, new_draft,
                );
                let mut fetch_items = vec![("FLAGS".to_string(), flags_str)];
                if uid_mode {
                    fetch_items.push(("UID".to_string(), msg_uid.to_string()));
                }
                response.push_str(&ImapResponse::fetch(seq, &fetch_items));
            }
        }

        response.push_str(&ImapResponse::ok(tag, "STORE completed"));
        response
    }

    /// Apply flag changes based on store operation
    fn apply_flag_changes(msg: &Message, flags: &StoreFlags) -> (bool, bool, bool, bool, bool) {
        let mut seen = msg.seen;
        let mut answered = msg.answered;
        let mut flagged = msg.flagged;
        let mut deleted = msg.deleted;
        let mut draft = msg.draft;

        for flag in &flags.flags {
            let flag_upper = flag.to_uppercase();
            let value = match flags.operation {
                StoreOperation::Add => true,
                StoreOperation::Remove => false,
                StoreOperation::Replace => true,
            };

            match flag_upper.as_str() {
                "\\SEEN" => {
                    if flags.operation == StoreOperation::Replace {
                        seen = flags.flags.iter().any(|f| f.to_uppercase() == "\\SEEN");
                    } else {
                        seen = value;
                    }
                }
                "\\ANSWERED" => {
                    if flags.operation == StoreOperation::Replace {
                        answered = flags.flags.iter().any(|f| f.to_uppercase() == "\\ANSWERED");
                    } else {
                        answered = value;
                    }
                }
                "\\FLAGGED" => {
                    if flags.operation == StoreOperation::Replace {
                        flagged = flags.flags.iter().any(|f| f.to_uppercase() == "\\FLAGGED");
                    } else {
                        flagged = value;
                    }
                }
                "\\DELETED" => {
                    if flags.operation == StoreOperation::Replace {
                        deleted = flags.flags.iter().any(|f| f.to_uppercase() == "\\DELETED");
                    } else {
                        deleted = value;
                    }
                }
                "\\DRAFT" => {
                    if flags.operation == StoreOperation::Replace {
                        draft = flags.flags.iter().any(|f| f.to_uppercase() == "\\DRAFT");
                    } else {
                        draft = value;
                    }
                }
                _ => {}
            }
        }

        // For Replace operation, reset flags not in the list
        if flags.operation == StoreOperation::Replace {
            if !flags.flags.iter().any(|f| f.to_uppercase() == "\\SEEN") {
                seen = false;
            }
            if !flags.flags.iter().any(|f| f.to_uppercase() == "\\ANSWERED") {
                answered = false;
            }
            if !flags.flags.iter().any(|f| f.to_uppercase() == "\\FLAGGED") {
                flagged = false;
            }
            if !flags.flags.iter().any(|f| f.to_uppercase() == "\\DELETED") {
                deleted = false;
            }
            if !flags.flags.iter().any(|f| f.to_uppercase() == "\\DRAFT") {
                draft = false;
            }
        }

        (seen, answered, flagged, deleted, draft)
    }

    /// Handle COPY command
    async fn handle_copy(
        tag: &str,
        sequence: &SequenceSet,
        dest_mailbox: &str,
        uid_mode: bool,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_selected() {
            return ImapResponse::no(tag, "No mailbox selected");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };

        let selected = match &sess.selected_mailbox {
            Some(s) => s.clone(),
            None => return ImapResponse::no(tag, "No mailbox selected"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Find destination mailbox
        let dest_mailbox_query = if dest_mailbox.to_uppercase() == "INBOX" {
            sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM mailboxes WHERE tenant_id = $1 LIMIT 1",
            )
            .bind(tenant_id)
        } else {
            sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM mailboxes WHERE tenant_id = $1 AND address = $2",
            )
            .bind(tenant_id)
            .bind(dest_mailbox)
        };

        let dest_id = match dest_mailbox_query.fetch_optional(pool).await {
            Ok(Some((id,))) => id,
            Ok(None) => return ImapResponse::no(tag, "[TRYCREATE] Destination mailbox does not exist"),
            Err(e) => {
                error!("Failed to find destination mailbox: {}", e);
                return ImapResponse::no(tag, "Failed to find destination mailbox");
            }
        };

        // Get source messages
        let messages: Vec<Message> = sqlx::query_as(
            "SELECT * FROM messages WHERE mailbox_id = $1 ORDER BY received_at ASC",
        )
        .bind(selected.id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let max_seq = messages.len() as u32;
        let mut source_uids = Vec::new();
        let mut dest_uids = Vec::new();

        for (idx, msg) in messages.iter().enumerate() {
            let seq = (idx + 1) as u32;
            let msg_uid = Self::message_id_to_uid(&msg.id);

            let in_set = if uid_mode {
                sequence.contains(msg_uid, msg_uid)
            } else {
                sequence.contains(seq, max_seq)
            };

            if !in_set {
                continue;
            }

            // Copy the message
            let new_id = Uuid::new_v4();
            let new_uid = Self::message_id_to_uid(&new_id);

            let copy_result = sqlx::query(
                "INSERT INTO messages (id, tenant_id, mailbox_id, message_id_header, subject,
                 from_address, to_addresses, cc_addresses, headers, body_preview, body_size,
                 has_attachments, storage_path, seen, answered, flagged, deleted, draft,
                 spam_score, tags, metadata, received_at, created_at)
                 SELECT $1, tenant_id, $2, message_id_header, subject, from_address, to_addresses,
                 cc_addresses, headers, body_preview, body_size, has_attachments, storage_path,
                 seen, answered, flagged, false, draft, spam_score, tags, metadata, received_at, NOW()
                 FROM messages WHERE id = $3",
            )
            .bind(new_id)
            .bind(dest_id)
            .bind(msg.id)
            .execute(pool)
            .await;

            match copy_result {
                Ok(_) => {
                    source_uids.push(msg_uid.to_string());
                    dest_uids.push(new_uid.to_string());
                }
                Err(e) => {
                    error!("Failed to copy message: {}", e);
                }
            }
        }

        if source_uids.is_empty() {
            return ImapResponse::ok(tag, "COPY completed (no messages)");
        }

        let copyuid = ImapResponse::copyuid(
            selected.uid_validity,
            &source_uids.join(","),
            &dest_uids.join(","),
        );
        ImapResponse::ok(tag, &format!("{} COPY completed", copyuid))
    }

    /// Handle MOVE command
    async fn handle_move(
        tag: &str,
        sequence: &SequenceSet,
        dest_mailbox: &str,
        uid_mode: bool,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_selected() {
            return ImapResponse::no(tag, "No mailbox selected");
        }

        if sess.is_readonly() {
            return ImapResponse::no(tag, "Mailbox is read-only");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };

        let selected = match &sess.selected_mailbox {
            Some(s) => s.clone(),
            None => return ImapResponse::no(tag, "No mailbox selected"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Find destination mailbox
        let dest_mailbox_query = if dest_mailbox.to_uppercase() == "INBOX" {
            sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM mailboxes WHERE tenant_id = $1 LIMIT 1",
            )
            .bind(tenant_id)
        } else {
            sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM mailboxes WHERE tenant_id = $1 AND address = $2",
            )
            .bind(tenant_id)
            .bind(dest_mailbox)
        };

        let dest_id = match dest_mailbox_query.fetch_optional(pool).await {
            Ok(Some((id,))) => id,
            Ok(None) => return ImapResponse::no(tag, "[TRYCREATE] Destination mailbox does not exist"),
            Err(e) => {
                error!("Failed to find destination mailbox: {}", e);
                return ImapResponse::no(tag, "Failed to find destination mailbox");
            }
        };

        // Get source messages
        let messages: Vec<Message> = sqlx::query_as(
            "SELECT * FROM messages WHERE mailbox_id = $1 ORDER BY received_at ASC",
        )
        .bind(selected.id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let max_seq = messages.len() as u32;
        let mut response = String::new();
        let mut source_uids = Vec::new();
        let mut dest_uids = Vec::new();
        let mut expunged_seqs = Vec::new();

        for (idx, msg) in messages.iter().enumerate() {
            let seq = (idx + 1) as u32;
            let msg_uid = Self::message_id_to_uid(&msg.id);

            let in_set = if uid_mode {
                sequence.contains(msg_uid, msg_uid)
            } else {
                sequence.contains(seq, max_seq)
            };

            if !in_set {
                continue;
            }

            // Move the message (update mailbox_id)
            let move_result = sqlx::query(
                "UPDATE messages SET mailbox_id = $2 WHERE id = $1",
            )
            .bind(msg.id)
            .bind(dest_id)
            .execute(pool)
            .await;

            match move_result {
                Ok(_) => {
                    source_uids.push(msg_uid.to_string());
                    dest_uids.push(msg_uid.to_string()); // UID doesn't change on move
                    expunged_seqs.push(seq);
                }
                Err(e) => {
                    error!("Failed to move message: {}", e);
                }
            }
        }

        // Send EXPUNGE responses for moved messages (in reverse order to maintain sequence numbers)
        for seq in expunged_seqs.iter().rev() {
            response.push_str(&ImapResponse::expunge(*seq));
        }

        if source_uids.is_empty() {
            response.push_str(&ImapResponse::ok(tag, "MOVE completed (no messages)"));
        } else {
            let copyuid = ImapResponse::copyuid(
                selected.uid_validity,
                &source_uids.join(","),
                &dest_uids.join(","),
            );
            response.push_str(&ImapResponse::ok(tag, &format!("{} MOVE completed", copyuid)));
        }

        response
    }

    /// Handle EXPUNGE command
    async fn handle_expunge(
        tag: &str,
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_selected() {
            return ImapResponse::no(tag, "No mailbox selected");
        }

        if sess.is_readonly() {
            return ImapResponse::no(tag, "Mailbox is read-only");
        }

        let selected = match &sess.selected_mailbox {
            Some(s) => s.clone(),
            None => return ImapResponse::no(tag, "No mailbox selected"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Get messages marked for deletion
        let messages: Vec<Message> = sqlx::query_as(
            "SELECT * FROM messages WHERE mailbox_id = $1 ORDER BY received_at ASC",
        )
        .bind(selected.id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut response = String::new();
        let mut deleted_count = 0;

        // Process in reverse order to maintain correct sequence numbers
        let mut seq_to_delete = Vec::new();
        for (idx, msg) in messages.iter().enumerate() {
            if msg.deleted {
                seq_to_delete.push((idx + 1) as u32);
            }
        }

        // Delete messages and send EXPUNGE responses
        for (offset, (idx, msg)) in messages.iter().enumerate().filter(|(_, m)| m.deleted).enumerate() {
            let seq = (idx + 1 - offset) as u32; // Adjust for already deleted messages

            let delete_result = sqlx::query("DELETE FROM messages WHERE id = $1")
                .bind(msg.id)
                .execute(pool)
                .await;

            if delete_result.is_ok() {
                response.push_str(&ImapResponse::expunge(seq));
                deleted_count += 1;
            }
        }

        info!("Expunged {} messages from mailbox {}", deleted_count, selected.name);
        response.push_str(&ImapResponse::ok(tag, "EXPUNGE completed"));
        response
    }

    /// Handle APPEND command
    async fn handle_append(
        tag: &str,
        mailbox_name: &str,
        flags: &[String],
        _date: Option<&str>,
        message: &[u8],
        session: &Arc<Mutex<ImapSession>>,
        db_pool: &DatabasePool,
        storage_path_base: &PathBuf,
    ) -> String {
        let sess = session.lock().await;
        if !sess.is_authenticated() {
            return ImapResponse::no(tag, "Not authenticated");
        }

        let tenant_id = match sess.tenant_id {
            Some(id) => id,
            None => return ImapResponse::no(tag, "No tenant context"),
        };
        drop(sess);

        let pool = db_pool.pool();

        // Find the mailbox
        let mailbox_query = if mailbox_name.to_uppercase() == "INBOX" {
            sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM mailboxes WHERE tenant_id = $1 LIMIT 1",
            )
            .bind(tenant_id)
        } else {
            sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM mailboxes WHERE tenant_id = $1 AND address = $2",
            )
            .bind(tenant_id)
            .bind(mailbox_name)
        };

        let mailbox_id = match mailbox_query.fetch_optional(pool).await {
            Ok(Some((id,))) => id,
            Ok(None) => return ImapResponse::no(tag, "[TRYCREATE] Mailbox does not exist"),
            Err(e) => {
                error!("Failed to find mailbox: {}", e);
                return ImapResponse::no(tag, "Failed to find mailbox");
            }
        };

        // Parse flags
        let seen = flags.iter().any(|f| f.to_uppercase() == "\\SEEN");
        let answered = flags.iter().any(|f| f.to_uppercase() == "\\ANSWERED");
        let flagged = flags.iter().any(|f| f.to_uppercase() == "\\FLAGGED");
        let deleted = flags.iter().any(|f| f.to_uppercase() == "\\DELETED");
        let draft = flags.iter().any(|f| f.to_uppercase() == "\\DRAFT");

        // Create the message
        let message_id = Uuid::new_v4();
        // Create preview from first 500 characters of message for quick display
        let body_preview = String::from_utf8_lossy(message).chars().take(500).collect::<String>();
        let storage_path = format!("{}/{}/{}.eml", tenant_id, mailbox_id, message_id);

        // Initialize file storage and store the FULL message
        let storage = match LocalStorage::from_path(storage_path_base) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to initialize storage for APPEND: {}", e);
                return ImapResponse::no(tag, "Failed to initialize storage");
            }
        };

        // Write the full message to file storage
        if let Err(e) = storage.store(&storage_path, message).await {
            error!("Failed to store message to file: {}", e);
            return ImapResponse::no(tag, "Failed to store message");
        }

        let insert_result = sqlx::query(
            "INSERT INTO messages (id, tenant_id, mailbox_id, body_preview, body_size, storage_path,
             seen, answered, flagged, deleted, draft, to_addresses, headers, tags, metadata,
             received_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, '[]', '{}', '[]', '{}', NOW(), NOW())",
        )
        .bind(message_id)
        .bind(tenant_id)
        .bind(mailbox_id)
        .bind(&body_preview)
        .bind(message.len() as i64)
        .bind(&storage_path)
        .bind(seen)
        .bind(answered)
        .bind(flagged)
        .bind(deleted)
        .bind(draft)
        .execute(pool)
        .await;

        match insert_result {
            Ok(_) => {
                let uid = Self::message_id_to_uid(&message_id);
                let appenduid = ImapResponse::appenduid(1, uid);
                info!("Appended message {} to mailbox {}", message_id, mailbox_name);
                ImapResponse::ok(tag, &format!("{} APPEND completed", appenduid))
            }
            Err(e) => {
                error!("Failed to append message: {}", e);
                ImapResponse::no(tag, "Failed to append message")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ImapConfig::default();
        assert_eq!(config.bind, "0.0.0.0:143");
        assert!(!config.starttls);
        assert_eq!(config.timeout_minutes, 30);
    }
}
