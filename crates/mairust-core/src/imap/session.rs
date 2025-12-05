//! IMAP Session management
//!
//! Manages the state of an IMAP connection including authentication
//! and selected mailbox state.

use chrono::{DateTime, Utc};
use mairust_common::types::{MailboxId, TenantId, UserId};
use mairust_storage::models::Message;
use std::collections::HashMap;
use uuid::Uuid;

/// IMAP session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Not authenticated
    NotAuthenticated,
    /// Authenticated but no mailbox selected
    Authenticated,
    /// Mailbox selected for read-write
    Selected,
    /// Mailbox selected for read-only (EXAMINE)
    ReadOnly,
    /// Session is closing
    Logout,
}

/// Selected mailbox information
#[derive(Debug, Clone)]
pub struct SelectedMailbox {
    /// Mailbox ID
    pub id: MailboxId,
    /// Mailbox name
    pub name: String,
    /// Total message count
    pub exists: u32,
    /// Recent message count
    pub recent: u32,
    /// First unseen message sequence number
    pub first_unseen: Option<u32>,
    /// UID validity value
    pub uid_validity: u32,
    /// Next UID value
    pub uid_next: u32,
    /// Available flags
    pub flags: Vec<String>,
    /// Message UID to sequence number mapping
    pub uid_map: HashMap<u32, u32>,
    /// Sequence number to message ID mapping
    pub seq_to_id: HashMap<u32, Uuid>,
}

impl SelectedMailbox {
    /// Create a new selected mailbox
    pub fn new(id: MailboxId, name: String) -> Self {
        Self {
            id,
            name,
            exists: 0,
            recent: 0,
            first_unseen: None,
            uid_validity: 1,
            uid_next: 1,
            flags: vec![
                "\\Seen".to_string(),
                "\\Answered".to_string(),
                "\\Flagged".to_string(),
                "\\Deleted".to_string(),
                "\\Draft".to_string(),
            ],
            uid_map: HashMap::new(),
            seq_to_id: HashMap::new(),
        }
    }

    /// Update mailbox with messages
    pub fn update_with_messages(&mut self, messages: &[Message]) {
        self.exists = messages.len() as u32;
        self.uid_map.clear();
        self.seq_to_id.clear();

        let mut first_unseen = None;
        let mut recent_count = 0u32;
        let mut max_uid = 0u32;

        for (idx, msg) in messages.iter().enumerate() {
            let seq = (idx + 1) as u32;
            // Use message ID bytes as UID (simplified)
            let uid = Self::message_id_to_uid(&msg.id);

            self.uid_map.insert(uid, seq);
            self.seq_to_id.insert(seq, msg.id);

            if !msg.seen && first_unseen.is_none() {
                first_unseen = Some(seq);
            }

            // Count recent messages (simplified: check if created today)
            let now = Utc::now();
            if (now - msg.created_at).num_hours() < 24 {
                recent_count += 1;
            }

            if uid > max_uid {
                max_uid = uid;
            }
        }

        self.first_unseen = first_unseen;
        self.recent = recent_count;
        self.uid_next = max_uid.saturating_add(1);
    }

    /// Convert message ID to UID
    fn message_id_to_uid(id: &Uuid) -> u32 {
        // Use first 4 bytes of UUID as UID
        let bytes = id.as_bytes();
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    /// Get message ID by sequence number
    pub fn get_message_id(&self, seq: u32) -> Option<Uuid> {
        self.seq_to_id.get(&seq).copied()
    }

    /// Get message ID by UID
    pub fn get_message_id_by_uid(&self, uid: u32) -> Option<Uuid> {
        let seq = self.uid_map.get(&uid)?;
        self.seq_to_id.get(seq).copied()
    }

    /// Get sequence number by UID
    pub fn get_seq_by_uid(&self, uid: u32) -> Option<u32> {
        self.uid_map.get(&uid).copied()
    }
}

/// IMAP Session
#[derive(Debug)]
pub struct ImapSession {
    /// Session ID
    pub id: String,
    /// Current state
    pub state: SessionState,
    /// Authenticated user ID
    pub user_id: Option<UserId>,
    /// Authenticated tenant ID
    pub tenant_id: Option<TenantId>,
    /// User email address
    pub user_email: Option<String>,
    /// Currently selected mailbox
    pub selected_mailbox: Option<SelectedMailbox>,
    /// Session start time
    pub started_at: DateTime<Utc>,
    /// Last activity time
    pub last_activity: DateTime<Utc>,
}

impl ImapSession {
    /// Create a new session
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            state: SessionState::NotAuthenticated,
            user_id: None,
            tenant_id: None,
            user_email: None,
            selected_mailbox: None,
            started_at: now,
            last_activity: now,
        }
    }

    /// Check if session is authenticated
    pub fn is_authenticated(&self) -> bool {
        matches!(
            self.state,
            SessionState::Authenticated | SessionState::Selected | SessionState::ReadOnly
        )
    }

    /// Check if a mailbox is selected
    pub fn is_selected(&self) -> bool {
        matches!(self.state, SessionState::Selected | SessionState::ReadOnly)
    }

    /// Check if in read-only mode
    pub fn is_readonly(&self) -> bool {
        matches!(self.state, SessionState::ReadOnly)
    }

    /// Set authenticated state
    pub fn authenticate(&mut self, user_id: UserId, tenant_id: TenantId, email: String) {
        self.user_id = Some(user_id);
        self.tenant_id = Some(tenant_id);
        self.user_email = Some(email);
        self.state = SessionState::Authenticated;
        self.update_activity();
    }

    /// Select a mailbox
    pub fn select(&mut self, mailbox: SelectedMailbox, readonly: bool) {
        self.selected_mailbox = Some(mailbox);
        self.state = if readonly {
            SessionState::ReadOnly
        } else {
            SessionState::Selected
        };
        self.update_activity();
    }

    /// Close the selected mailbox
    pub fn close_mailbox(&mut self) {
        self.selected_mailbox = None;
        self.state = SessionState::Authenticated;
        self.update_activity();
    }

    /// Set logout state
    pub fn logout(&mut self) {
        self.state = SessionState::Logout;
    }

    /// Update last activity timestamp
    pub fn update_activity(&mut self) {
        self.last_activity = Utc::now();
    }

    /// Check if session has timed out (default: 30 minutes)
    pub fn is_timed_out(&self, timeout_minutes: i64) -> bool {
        let elapsed = Utc::now() - self.last_activity;
        elapsed.num_minutes() > timeout_minutes
    }
}

impl Default for ImapSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = ImapSession::new();
        assert_eq!(session.state, SessionState::NotAuthenticated);
        assert!(!session.is_authenticated());
        assert!(!session.is_selected());
    }

    #[test]
    fn test_session_authenticate() {
        let mut session = ImapSession::new();
        session.authenticate(Uuid::new_v4(), Uuid::new_v4(), "user@example.com".to_string());
        assert_eq!(session.state, SessionState::Authenticated);
        assert!(session.is_authenticated());
        assert!(!session.is_selected());
    }

    #[test]
    fn test_session_select() {
        let mut session = ImapSession::new();
        session.authenticate(Uuid::new_v4(), Uuid::new_v4(), "user@example.com".to_string());

        let mailbox = SelectedMailbox::new(Uuid::new_v4(), "INBOX".to_string());
        session.select(mailbox, false);

        assert_eq!(session.state, SessionState::Selected);
        assert!(session.is_selected());
        assert!(!session.is_readonly());
    }

    #[test]
    fn test_session_examine() {
        let mut session = ImapSession::new();
        session.authenticate(Uuid::new_v4(), Uuid::new_v4(), "user@example.com".to_string());

        let mailbox = SelectedMailbox::new(Uuid::new_v4(), "INBOX".to_string());
        session.select(mailbox, true);

        assert_eq!(session.state, SessionState::ReadOnly);
        assert!(session.is_selected());
        assert!(session.is_readonly());
    }

    #[test]
    fn test_selected_mailbox() {
        let mut mailbox = SelectedMailbox::new(Uuid::new_v4(), "INBOX".to_string());
        assert_eq!(mailbox.exists, 0);
        assert_eq!(mailbox.flags.len(), 5);
    }
}
