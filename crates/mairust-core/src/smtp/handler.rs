//! SMTP session handler

use crate::hooks::HookManager;
use crate::queue::QueueManager;
use crate::smtp::auth::{login_challenge_password, login_challenge_username, SmtpAuthenticator};
use anyhow::Result;
use chrono::Utc;
use mairust_common::config::SmtpConfig;
use mairust_common::types::{EmailAddress, Envelope};
use mairust_storage::db::DatabasePool;
use mairust_storage::file::FileStorage;
use mairust_storage::models::{Message, User};
use mairust_storage::repository::{DomainRepository, MailboxRepository, MessageRepository};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// SMTP session state
#[derive(Debug, Clone, PartialEq)]
enum SessionState {
    Connected,
    Greeted,
    MailFrom,
    RcptTo,
    Data,
}

/// Result of command processing
enum CommandResult {
    Continue,
    Quit,
    StartTls,
}

/// SMTP session handler
pub struct SmtpHandler<S: FileStorage> {
    config: SmtpConfig,
    db_pool: DatabasePool,
    file_storage: Arc<S>,
    hook_manager: Arc<HookManager>,
    #[allow(dead_code)]
    queue_manager: Arc<QueueManager<S>>,
    peer_addr: SocketAddr,
}

impl<S: FileStorage + Send + Sync + 'static> SmtpHandler<S> {
    /// Create a new handler
    pub fn new(
        config: SmtpConfig,
        db_pool: DatabasePool,
        file_storage: Arc<S>,
        hook_manager: Arc<HookManager>,
        queue_manager: Arc<QueueManager<S>>,
        peer_addr: SocketAddr,
    ) -> Self {
        Self {
            config,
            db_pool,
            file_storage,
            hook_manager,
            queue_manager,
            peer_addr,
        }
    }

    /// Handle an SMTP session (legacy method without TLS)
    pub async fn handle(self, stream: TcpStream) -> Result<()> {
        self.handle_with_tls(stream, None).await
    }

    /// Handle an SMTP session with optional TLS support
    pub async fn handle_with_tls(
        self,
        stream: TcpStream,
        tls_acceptor: Option<Arc<TlsAcceptor>>,
    ) -> Result<()> {
        // Start with plain text session
        let (reader, writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut writer = BufWriter::new(writer);

        // Run the session
        let result = self
            .run_session(&mut reader, &mut writer, false, tls_acceptor.clone())
            .await;

        match result {
            Ok(Some(tcp_stream)) => {
                // STARTTLS was requested, upgrade to TLS
                if let Some(acceptor) = tls_acceptor {
                    info!("Upgrading connection to TLS for {}", self.peer_addr);
                    match acceptor.accept(tcp_stream).await {
                        Ok(tls_stream) => {
                            let (tls_reader, tls_writer) = tokio::io::split(tls_stream);
                            let mut tls_reader = BufReader::new(tls_reader);
                            let mut tls_writer = BufWriter::new(tls_writer);

                            // Continue session over TLS
                            if let Err(e) = self
                                .run_session(&mut tls_reader, &mut tls_writer, true, None)
                                .await
                            {
                                error!("TLS session error: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("TLS handshake failed for {}: {}", self.peer_addr, e);
                        }
                    }
                }
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Run the SMTP session
    /// Returns Ok(Some(stream)) if STARTTLS was requested and we need to upgrade
    /// Returns Ok(None) if session ended normally
    async fn run_session<R, W>(
        &self,
        reader: &mut BufReader<R>,
        writer: &mut BufWriter<W>,
        tls_established: bool,
        _tls_acceptor: Option<Arc<TlsAcceptor>>,
    ) -> Result<Option<TcpStream>>
    where
        R: tokio::io::AsyncRead + Unpin,
        W: tokio::io::AsyncWrite + Unpin,
    {
        let mut state = SessionState::Connected;
        let mut envelope = Envelope {
            from: None,
            to: Vec::new(),
            client_ip: Some(self.peer_addr.ip().to_string()),
            helo: None,
        };
        let mut authenticated = false;
        #[allow(unused_assignments)]
        let mut authenticated_user: Option<User> = None;
        let authenticator = SmtpAuthenticator::new(self.db_pool.clone());

        // Send greeting
        self.send_response(writer, 220, &format!("{} ESMTP MaiRust", self.config.hostname))
            .await?;

        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                debug!("Client {} disconnected", self.peer_addr);
                break;
            }

            let cmd_line = line.trim().to_string();
            debug!("SMTP from {}: {}", self.peer_addr, cmd_line);

            let (command, args) = parse_command(&cmd_line);
            let command = command.to_string();
            let args = args.to_string();

            let result = self
                .process_command(
                    &command,
                    &args,
                    &mut state,
                    &mut envelope,
                    &mut authenticated,
                    &mut authenticated_user,
                    &authenticator,
                    tls_established,
                    reader,
                    writer,
                    &mut line,
                )
                .await?;

            match result {
                CommandResult::Continue => continue,
                CommandResult::Quit => break,
                CommandResult::StartTls => {
                    // STARTTLS requested - we can't return the stream here
                    // because we don't own it anymore. The caller needs to handle this.
                    // For now, just log and continue (full implementation would require
                    // refactoring to pass ownership back)
                    warn!("STARTTLS upgrade not fully implemented in this code path");
                    break;
                }
            }
        }

        Ok(None)
    }

    /// Process a single SMTP command
    #[allow(clippy::too_many_arguments)]
    async fn process_command<R, W>(
        &self,
        command: &str,
        args: &str,
        state: &mut SessionState,
        envelope: &mut Envelope,
        authenticated: &mut bool,
        authenticated_user: &mut Option<User>,
        authenticator: &SmtpAuthenticator,
        tls_established: bool,
        reader: &mut R,
        writer: &mut W,
        line: &mut String,
    ) -> Result<CommandResult>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        match command.to_uppercase().as_str() {
            "HELO" => {
                envelope.helo = Some(args.to_string());
                *state = SessionState::Greeted;
                self.send_response(writer, 250, &format!("Hello {}", args))
                    .await?;
            }

            "EHLO" => {
                envelope.helo = Some(args.to_string());
                *state = SessionState::Greeted;

                // Send EHLO response with extensions
                let mut responses = vec![
                    format!("{} Hello {}", self.config.hostname, args),
                    "SIZE 52428800".to_string(), // 50MB
                    "8BITMIME".to_string(),
                    "PIPELINING".to_string(),
                    "ENHANCEDSTATUSCODES".to_string(),
                ];

                // Only advertise STARTTLS if TLS is enabled and not already established
                if self.config.tls_enabled.unwrap_or(false) && !tls_established {
                    responses.push("STARTTLS".to_string());
                }

                // Advertise AUTH
                if self.config.auth_required.unwrap_or(false) || *authenticated {
                    responses.push("AUTH PLAIN LOGIN".to_string());
                }

                for (i, resp) in responses.iter().enumerate() {
                    if i == responses.len() - 1 {
                        self.send_response(writer, 250, resp).await?;
                    } else {
                        self.send_response_continue(writer, 250, resp).await?;
                    }
                }
            }

            "STARTTLS" => {
                if !self.config.tls_enabled.unwrap_or(false) {
                    self.send_response(writer, 502, "5.5.1 STARTTLS not supported")
                        .await?;
                    return Ok(CommandResult::Continue);
                }

                if tls_established {
                    self.send_response(writer, 503, "5.5.1 TLS already active")
                        .await?;
                    return Ok(CommandResult::Continue);
                }

                self.send_response(writer, 220, "2.0.0 Ready to start TLS")
                    .await?;

                return Ok(CommandResult::StartTls);
            }

            "AUTH" => {
                if *state != SessionState::Greeted {
                    self.send_response(writer, 503, "5.5.1 Bad sequence of commands")
                        .await?;
                    return Ok(CommandResult::Continue);
                }

                // Check if TLS is required for authentication
                if self.config.require_tls_for_auth && !tls_established {
                    self.send_response(
                        writer,
                        538,
                        "5.7.11 Encryption required for requested authentication mechanism",
                    )
                    .await?;
                    return Ok(CommandResult::Continue);
                }

                let auth_parts: Vec<&str> = args.splitn(2, ' ').collect();
                let mechanism = auth_parts.first().map(|s| s.to_uppercase());

                match mechanism.as_deref() {
                    Some("PLAIN") => {
                        // AUTH PLAIN [initial-response]
                        let credentials = if let Some(initial_response) = auth_parts.get(1) {
                            // Credentials provided inline
                            initial_response.to_string()
                        } else {
                            // Request credentials
                            self.send_response(writer, 334, "").await?;

                            // Read credentials
                            line.clear();
                            let bytes_read = reader.read_line(line).await?;
                            if bytes_read == 0 {
                                return Ok(CommandResult::Quit);
                            }
                            line.trim().to_string()
                        };

                        // Handle cancel
                        if credentials == "*" {
                            self.send_response(writer, 501, "5.7.0 Authentication cancelled")
                                .await?;
                            return Ok(CommandResult::Continue);
                        }

                        // Authenticate
                        let result = authenticator.authenticate_plain(&credentials).await;
                        if result.success {
                            *authenticated = true;
                            *authenticated_user = result.user;
                            info!(
                                "SMTP AUTH PLAIN successful for {:?} from {}",
                                authenticated_user.as_ref().map(|u| &u.email),
                                self.peer_addr
                            );
                            self.send_response(writer, 235, "2.7.0 Authentication successful")
                                .await?;
                        } else {
                            warn!(
                                "SMTP AUTH PLAIN failed from {}: {:?}",
                                self.peer_addr, result.error
                            );
                            self.send_response(
                                writer,
                                535,
                                "5.7.8 Authentication credentials invalid",
                            )
                            .await?;
                        }
                    }
                    Some("LOGIN") => {
                        // AUTH LOGIN flow - challenge/response
                        // Send username challenge
                        self.send_response(writer, 334, &login_challenge_username())
                            .await?;

                        // Read username
                        line.clear();
                        let bytes_read = reader.read_line(line).await?;
                        if bytes_read == 0 {
                            return Ok(CommandResult::Quit);
                        }
                        let username = line.trim().to_string();

                        // Handle cancel
                        if username == "*" {
                            self.send_response(writer, 501, "5.7.0 Authentication cancelled")
                                .await?;
                            return Ok(CommandResult::Continue);
                        }

                        // Send password challenge
                        self.send_response(writer, 334, &login_challenge_password())
                            .await?;

                        // Read password
                        line.clear();
                        let bytes_read = reader.read_line(line).await?;
                        if bytes_read == 0 {
                            return Ok(CommandResult::Quit);
                        }
                        let password = line.trim().to_string();

                        // Handle cancel
                        if password == "*" {
                            self.send_response(writer, 501, "5.7.0 Authentication cancelled")
                                .await?;
                            return Ok(CommandResult::Continue);
                        }

                        // Authenticate
                        let result = authenticator.authenticate_login(&username, &password).await;
                        if result.success {
                            *authenticated = true;
                            *authenticated_user = result.user;
                            info!(
                                "SMTP AUTH LOGIN successful for {:?} from {}",
                                authenticated_user.as_ref().map(|u| &u.email),
                                self.peer_addr
                            );
                            self.send_response(writer, 235, "2.7.0 Authentication successful")
                                .await?;
                        } else {
                            warn!(
                                "SMTP AUTH LOGIN failed from {}: {:?}",
                                self.peer_addr, result.error
                            );
                            self.send_response(
                                writer,
                                535,
                                "5.7.8 Authentication credentials invalid",
                            )
                            .await?;
                        }
                    }
                    _ => {
                        self.send_response(
                            writer,
                            504,
                            "5.5.4 Unrecognized authentication mechanism",
                        )
                        .await?;
                    }
                }
            }

            "MAIL" => {
                if *state != SessionState::Greeted {
                    self.send_response(writer, 503, "5.5.1 Bad sequence of commands")
                        .await?;
                    return Ok(CommandResult::Continue);
                }

                // Check if auth is required for submission port
                if self.config.auth_required.unwrap_or(false) && !*authenticated {
                    self.send_response(writer, 530, "5.7.0 Authentication required")
                        .await?;
                    return Ok(CommandResult::Continue);
                }

                // Parse MAIL FROM:<address>
                if let Some(from_addr) = parse_mail_from(args) {
                    envelope.from = from_addr;
                    *state = SessionState::MailFrom;
                    self.send_response(writer, 250, "2.1.0 OK").await?;
                } else {
                    self.send_response(writer, 501, "5.1.7 Bad sender address syntax")
                        .await?;
                }
            }

            "RCPT" => {
                if *state != SessionState::MailFrom && *state != SessionState::RcptTo {
                    self.send_response(writer, 503, "5.5.1 Bad sequence of commands")
                        .await?;
                    return Ok(CommandResult::Continue);
                }

                // Parse RCPT TO:<address>
                if let Some(to_addr) = parse_rcpt_to(args) {
                    // Check if we handle this domain
                    let domain_repo = DomainRepository::new(self.db_pool.clone());
                    match domain_repo.find_by_name(&to_addr.domain).await {
                        Ok(Some(_domain)) => {
                            envelope.to.push(to_addr);
                            *state = SessionState::RcptTo;
                            self.send_response(writer, 250, "2.1.5 OK").await?;
                        }
                        Ok(None) => {
                            // We don't handle this domain - relay not allowed
                            self.send_response(
                                writer,
                                550,
                                "5.1.1 Recipient address rejected: Domain not found",
                            )
                            .await?;
                        }
                        Err(e) => {
                            warn!("Database error checking domain: {}", e);
                            self.send_response(writer, 451, "4.3.0 Temporary error")
                                .await?;
                        }
                    }
                } else {
                    self.send_response(writer, 501, "5.1.3 Bad recipient address syntax")
                        .await?;
                }
            }

            "DATA" => {
                if *state != SessionState::RcptTo {
                    self.send_response(writer, 503, "5.5.1 Bad sequence of commands")
                        .await?;
                    return Ok(CommandResult::Continue);
                }

                if envelope.to.is_empty() {
                    self.send_response(writer, 503, "5.5.1 No recipients specified")
                        .await?;
                    return Ok(CommandResult::Continue);
                }

                let _ = SessionState::Data;
                self.send_response(writer, 354, "Start mail input; end with <CRLF>.<CRLF>")
                    .await?;

                // Read message data
                match self.read_data(reader).await {
                    Ok(data) => {
                        // Process the message
                        match self.process_message(envelope, &data).await {
                            Ok(message_id) => {
                                info!(
                                    "Message {} accepted from {} for {:?}",
                                    message_id, self.peer_addr, envelope.to
                                );
                                self.send_response(
                                    writer,
                                    250,
                                    &format!("2.0.0 OK: queued as {}", message_id),
                                )
                                .await?;
                            }
                            Err(e) => {
                                warn!("Failed to process message: {}", e);
                                self.send_response(writer, 451, "4.3.0 Temporary error")
                                    .await?;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read message data: {}", e);
                        self.send_response(writer, 451, "4.3.0 Error reading message")
                            .await?;
                    }
                }

                // Reset state for next message
                *state = SessionState::Greeted;
                envelope.from = None;
                envelope.to.clear();
            }

            "RSET" => {
                envelope.from = None;
                envelope.to.clear();
                if *state != SessionState::Connected {
                    *state = SessionState::Greeted;
                }
                self.send_response(writer, 250, "2.0.0 OK").await?;
            }

            "NOOP" => {
                self.send_response(writer, 250, "2.0.0 OK").await?;
            }

            "QUIT" => {
                self.send_response(writer, 221, "2.0.0 Bye").await?;
                return Ok(CommandResult::Quit);
            }

            "VRFY" => {
                self.send_response(writer, 252, "2.5.2 Cannot VRFY user")
                    .await?;
            }

            "EXPN" => {
                self.send_response(writer, 502, "5.5.1 EXPN not supported")
                    .await?;
            }

            _ => {
                self.send_response(writer, 500, "5.5.2 Command not recognized")
                    .await?;
            }
        }

        Ok(CommandResult::Continue)
    }

    /// Read message data until <CRLF>.<CRLF>
    async fn read_data<R: AsyncBufRead + Unpin>(&self, reader: &mut R) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut line = String::new();
        let max_size = self.config.max_message_size.unwrap_or(52_428_800); // 50MB default

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                return Err(anyhow::anyhow!("Connection closed during DATA"));
            }

            // Check for end of data
            if line.trim() == "." {
                break;
            }

            // Dot-stuffing: remove leading dot if followed by another dot
            let line_bytes = if line.starts_with("..") {
                &line.as_bytes()[1..]
            } else {
                line.as_bytes()
            };

            data.extend_from_slice(line_bytes);

            if data.len() > max_size {
                return Err(anyhow::anyhow!("Message too large"));
            }
        }

        Ok(data)
    }

    /// Process and store a received message
    async fn process_message(&self, envelope: &Envelope, data: &[u8]) -> Result<Uuid> {
        let message_id = Uuid::now_v7();

        // Parse message headers
        let parsed = mail_parser::MessageParser::default()
            .parse(data)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse message"))?;

        let subject = parsed.subject().map(|s| s.to_string());
        let from_header = parsed.from().and_then(|a| a.first()).map(|a| {
            if let Some(email) = a.address() {
                email.to_string()
            } else {
                String::new()
            }
        });
        let message_id_header = parsed.message_id().map(|s| s.to_string());

        // Get body preview
        let body_preview = parsed
            .body_text(0)
            .map(|s| s.chars().take(500).collect::<String>());

        // For each recipient, store the message
        for recipient in &envelope.to {
            // Find the mailbox for this recipient
            let mailbox_repo = MailboxRepository::new(self.db_pool.clone());

            let mailbox = match mailbox_repo.find_by_address(&recipient.to_string()).await? {
                Some(mb) => mb,
                None => {
                    warn!("Mailbox not found for {}", recipient);
                    continue;
                }
            };

            // Store the raw message to file storage
            let storage_path = format!(
                "{}/{}/{}.eml",
                mailbox.tenant_id, mailbox.id, message_id
            );

            self.file_storage.store(&storage_path, data).await?;

            // Create message record
            let message = Message {
                id: message_id,
                tenant_id: mailbox.tenant_id,
                mailbox_id: mailbox.id,
                message_id_header: message_id_header.clone(),
                subject: subject.clone(),
                from_address: from_header.clone(),
                to_addresses: serde_json::to_value(&envelope.to)?,
                cc_addresses: None,
                headers: serde_json::json!({}),
                body_preview: body_preview.clone(),
                body_size: data.len() as i64,
                has_attachments: parsed.attachment_count() > 0,
                storage_path: storage_path.clone(),
                seen: false,
                answered: false,
                flagged: false,
                deleted: false,
                draft: false,
                spam_score: None,
                tags: serde_json::json!([]),
                metadata: serde_json::json!({}),
                received_at: Utc::now(),
                created_at: Utc::now(),
            };

            // Store in database
            let message_repo = MessageRepository::new(self.db_pool.clone());
            message_repo.create(&message).await?;

            // Execute post_receive hooks
            if let Err(e) = self
                .hook_manager
                .execute_post_receive(mailbox.tenant_id, &message, data)
                .await
            {
                warn!("Hook execution failed for message {}: {}", message_id, e);
            }
        }

        Ok(message_id)
    }

    /// Send an SMTP response
    async fn send_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        code: u16,
        message: &str,
    ) -> Result<()> {
        let response = format!("{} {}\r\n", code, message);
        writer.write_all(response.as_bytes()).await?;
        writer.flush().await?;
        debug!("SMTP to {}: {}", self.peer_addr, response.trim());
        Ok(())
    }

    /// Send a multi-line response (intermediate line)
    async fn send_response_continue<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        code: u16,
        message: &str,
    ) -> Result<()> {
        let response = format!("{}-{}\r\n", code, message);
        writer.write_all(response.as_bytes()).await?;
        writer.flush().await?;
        debug!("SMTP to {}: {}", self.peer_addr, response.trim());
        Ok(())
    }
}

/// Parse an SMTP command line into command and arguments
fn parse_command(line: &str) -> (&str, &str) {
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    (parts.first().unwrap_or(&""), parts.get(1).unwrap_or(&""))
}

/// Parse MAIL FROM:<address> or MAIL FROM: <address>
fn parse_mail_from(args: &str) -> Option<Option<EmailAddress>> {
    let args = args.trim();

    // Check for FROM: prefix
    let addr_part = if args.to_uppercase().starts_with("FROM:") {
        &args[5..]
    } else {
        return None;
    };

    let addr_part = addr_part.trim();

    // Handle null sender <>
    if addr_part == "<>" {
        return Some(None);
    }

    // Extract address from angle brackets
    let email = if addr_part.starts_with('<') && addr_part.contains('>') {
        let end = addr_part.find('>')?;
        &addr_part[1..end]
    } else {
        addr_part.split_whitespace().next()?
    };

    if email.is_empty() {
        Some(None)
    } else {
        Some(EmailAddress::parse(email))
    }
}

/// Parse RCPT TO:<address>
fn parse_rcpt_to(args: &str) -> Option<EmailAddress> {
    let args = args.trim();

    // Check for TO: prefix
    let addr_part = if args.to_uppercase().starts_with("TO:") {
        &args[3..]
    } else {
        return None;
    };

    let addr_part = addr_part.trim();

    // Extract address from angle brackets
    let email = if addr_part.starts_with('<') && addr_part.contains('>') {
        let end = addr_part.find('>')?;
        &addr_part[1..end]
    } else {
        addr_part.split_whitespace().next()?
    };

    EmailAddress::parse(email)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mail_from() {
        assert_eq!(
            parse_mail_from("FROM:<user@example.com>"),
            Some(Some(EmailAddress::new("user", "example.com")))
        );

        assert_eq!(
            parse_mail_from("FROM: <user@example.com>"),
            Some(Some(EmailAddress::new("user", "example.com")))
        );

        assert_eq!(parse_mail_from("FROM:<>"), Some(None));

        assert_eq!(parse_mail_from("invalid"), None);
    }

    #[test]
    fn test_parse_rcpt_to() {
        assert_eq!(
            parse_rcpt_to("TO:<user@example.com>"),
            Some(EmailAddress::new("user", "example.com"))
        );

        assert_eq!(
            parse_rcpt_to("TO: <user@example.com>"),
            Some(EmailAddress::new("user", "example.com"))
        );

        assert_eq!(parse_rcpt_to("TO:<>"), None);
    }
}
