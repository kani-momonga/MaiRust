//! POP3 Session management
//!
//! Manages the state of a POP3 connection including authentication
//! and message state.

use chrono::{DateTime, Utc};
use mairust_common::types::{MailboxId, TenantId, UserId};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// POP3 session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Authorization state (not authenticated)
    Authorization,
    /// Transaction state (authenticated)
    Transaction,
    /// Update state (during QUIT processing)
    Update,
}

/// Message info for POP3 session
#[derive(Debug, Clone)]
pub struct MessageInfo {
    /// Message ID
    pub id: Uuid,
    /// Message size in bytes
    pub size: u64,
    /// Unique identifier string
    pub uid: String,
    /// Body preview (for quick access)
    pub body_preview: Option<String>,
    /// Storage path for full message
    pub storage_path: String,
}

/// POP3 Session
#[derive(Debug)]
pub struct Pop3Session {
    /// Session ID
    pub id: String,
    /// Current state
    pub state: SessionState,
    /// Username (provided via USER command)
    pub username: Option<String>,
    /// Authenticated user ID
    pub user_id: Option<UserId>,
    /// Authenticated tenant ID
    pub tenant_id: Option<TenantId>,
    /// Mailbox ID
    pub mailbox_id: Option<MailboxId>,
    /// Messages in the mailbox (1-indexed)
    pub messages: HashMap<u32, MessageInfo>,
    /// Messages marked for deletion
    pub deleted: HashSet<u32>,
    /// Session start time
    pub started_at: DateTime<Utc>,
    /// Last activity time
    pub last_activity: DateTime<Utc>,
}

impl Pop3Session {
    /// Create a new session
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            state: SessionState::Authorization,
            username: None,
            user_id: None,
            tenant_id: None,
            mailbox_id: None,
            messages: HashMap::new(),
            deleted: HashSet::new(),
            started_at: now,
            last_activity: now,
        }
    }

    /// Check if in authorization state
    pub fn is_authorization(&self) -> bool {
        matches!(self.state, SessionState::Authorization)
    }

    /// Check if in transaction state
    pub fn is_transaction(&self) -> bool {
        matches!(self.state, SessionState::Transaction)
    }

    /// Set username (USER command)
    pub fn set_username(&mut self, username: String) {
        self.username = Some(username);
        self.update_activity();
    }

    /// Authenticate the session
    pub fn authenticate(
        &mut self,
        user_id: UserId,
        tenant_id: TenantId,
        mailbox_id: MailboxId,
    ) {
        self.user_id = Some(user_id);
        self.tenant_id = Some(tenant_id);
        self.mailbox_id = Some(mailbox_id);
        self.state = SessionState::Transaction;
        self.update_activity();
    }

    /// Load messages into the session
    pub fn load_messages(&mut self, messages: Vec<MessageInfo>) {
        self.messages.clear();
        self.deleted.clear();
        for (idx, msg) in messages.into_iter().enumerate() {
            self.messages.insert((idx + 1) as u32, msg);
        }
    }

    /// Get message count (excluding deleted)
    pub fn message_count(&self) -> u32 {
        (self.messages.len() - self.deleted.len()) as u32
    }

    /// Get total size of messages (excluding deleted)
    pub fn total_size(&self) -> u64 {
        self.messages
            .iter()
            .filter(|(num, _)| !self.deleted.contains(num))
            .map(|(_, msg)| msg.size)
            .sum()
    }

    /// Get message by number (1-indexed)
    pub fn get_message(&self, num: u32) -> Option<&MessageInfo> {
        if self.deleted.contains(&num) {
            None
        } else {
            self.messages.get(&num)
        }
    }

    /// Mark message for deletion
    pub fn mark_deleted(&mut self, num: u32) -> bool {
        if self.messages.contains_key(&num) && !self.deleted.contains(&num) {
            self.deleted.insert(num);
            self.update_activity();
            true
        } else {
            false
        }
    }

    /// Reset deletions (RSET command)
    pub fn reset_deletions(&mut self) {
        self.deleted.clear();
        self.update_activity();
    }

    /// Get messages marked for deletion
    pub fn get_deleted_messages(&self) -> Vec<Uuid> {
        self.deleted
            .iter()
            .filter_map(|num| self.messages.get(num).map(|m| m.id))
            .collect()
    }

    /// Enter update state (QUIT command)
    pub fn enter_update(&mut self) {
        self.state = SessionState::Update;
    }

    /// Update last activity timestamp
    pub fn update_activity(&mut self) {
        self.last_activity = Utc::now();
    }

    /// Check if session has timed out
    pub fn is_timed_out(&self, timeout_minutes: i64) -> bool {
        let elapsed = Utc::now() - self.last_activity;
        elapsed.num_minutes() > timeout_minutes
    }

    /// Get list of messages for LIST command
    pub fn list_messages(&self) -> Vec<(u32, u64)> {
        self.messages
            .iter()
            .filter(|(num, _)| !self.deleted.contains(num))
            .map(|(num, msg)| (*num, msg.size))
            .collect()
    }

    /// Get list of UIDs for UIDL command
    pub fn uidl_messages(&self) -> Vec<(u32, String)> {
        self.messages
            .iter()
            .filter(|(num, _)| !self.deleted.contains(num))
            .map(|(num, msg)| (*num, msg.uid.clone()))
            .collect()
    }
}

impl Default for Pop3Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = Pop3Session::new();
        assert_eq!(session.state, SessionState::Authorization);
        assert!(session.is_authorization());
        assert!(!session.is_transaction());
    }

    #[test]
    fn test_message_operations() {
        let mut session = Pop3Session::new();

        let messages = vec![
            MessageInfo {
                id: Uuid::new_v4(),
                size: 100,
                uid: "uid1".to_string(),
                body_preview: None,
                storage_path: "/path/1".to_string(),
            },
            MessageInfo {
                id: Uuid::new_v4(),
                size: 200,
                uid: "uid2".to_string(),
                body_preview: None,
                storage_path: "/path/2".to_string(),
            },
        ];

        session.load_messages(messages);

        assert_eq!(session.message_count(), 2);
        assert_eq!(session.total_size(), 300);

        // Delete a message
        assert!(session.mark_deleted(1));
        assert_eq!(session.message_count(), 1);
        assert_eq!(session.total_size(), 200);

        // Can't delete already deleted
        assert!(!session.mark_deleted(1));

        // Reset
        session.reset_deletions();
        assert_eq!(session.message_count(), 2);
    }
}
