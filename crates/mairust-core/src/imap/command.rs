//! IMAP Command definitions
//!
//! Defines the IMAP commands supported by this server (read and write operations).

use serde::{Deserialize, Serialize};

/// IMAP command tag (client-provided identifier)
pub type Tag = String;

/// Sequence set for message selection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceSet {
    /// Single sequence number
    Single(u32),
    /// Range of sequence numbers (start:end, inclusive)
    Range(u32, u32),
    /// Wildcard (all messages)
    All,
    /// Multiple sets
    Multiple(Vec<SequenceSet>),
}

impl SequenceSet {
    /// Parse a sequence set string
    pub fn parse(s: &str) -> Option<Self> {
        if s == "*" {
            return Some(SequenceSet::All);
        }

        // Check for comma-separated sets
        if s.contains(',') {
            let parts: Vec<&str> = s.split(',').collect();
            let sets: Vec<SequenceSet> =
                parts.iter().filter_map(|p| Self::parse(p.trim())).collect();
            if sets.is_empty() {
                return None;
            }
            return Some(SequenceSet::Multiple(sets));
        }

        // Check for range
        if s.contains(':') {
            let parts: Vec<&str> = s.splitn(2, ':').collect();
            if parts.len() == 2 {
                let start = if parts[0] == "*" {
                    u32::MAX
                } else {
                    parts[0].parse().ok()?
                };
                let end = if parts[1] == "*" {
                    u32::MAX
                } else {
                    parts[1].parse().ok()?
                };
                return Some(SequenceSet::Range(start, end));
            }
            return None;
        }

        // Single number
        s.parse().ok().map(SequenceSet::Single)
    }

    /// Check if a sequence number is in this set
    pub fn contains(&self, seq: u32, max: u32) -> bool {
        match self {
            SequenceSet::Single(n) => {
                if *n == u32::MAX {
                    seq == max
                } else {
                    seq == *n
                }
            }
            SequenceSet::Range(start, end) => {
                let actual_start = if *start == u32::MAX { max } else { *start };
                let actual_end = if *end == u32::MAX { max } else { *end };
                seq >= actual_start && seq <= actual_end
            }
            SequenceSet::All => true,
            SequenceSet::Multiple(sets) => sets.iter().any(|s| s.contains(seq, max)),
        }
    }
}

/// FETCH data items
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchItem {
    /// Message flags
    Flags,
    /// Internal date
    InternalDate,
    /// RFC822.SIZE
    Rfc822Size,
    /// Envelope structure
    Envelope,
    /// BODY structure
    BodyStructure,
    /// Full body (UID optional)
    Body,
    /// Body section
    BodySection {
        section: String,
        partial: Option<(u32, u32)>,
    },
    /// BODY.PEEK section (doesn't set \Seen flag)
    BodyPeek {
        section: String,
        partial: Option<(u32, u32)>,
    },
    /// UID
    Uid,
    /// All standard attributes (FLAGS, INTERNALDATE, RFC822.SIZE, ENVELOPE)
    All,
    /// Fast attributes (FLAGS, INTERNALDATE, RFC822.SIZE)
    Fast,
    /// Full attributes (FLAGS, INTERNALDATE, RFC822.SIZE, ENVELOPE, BODY)
    Full,
}

impl FetchItem {
    /// Parse a single fetch item
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_uppercase();
        match s.as_str() {
            "FLAGS" => Some(FetchItem::Flags),
            "INTERNALDATE" => Some(FetchItem::InternalDate),
            "RFC822.SIZE" => Some(FetchItem::Rfc822Size),
            "ENVELOPE" => Some(FetchItem::Envelope),
            "BODYSTRUCTURE" => Some(FetchItem::BodyStructure),
            "BODY" => Some(FetchItem::Body),
            "UID" => Some(FetchItem::Uid),
            "ALL" => Some(FetchItem::All),
            "FAST" => Some(FetchItem::Fast),
            "FULL" => Some(FetchItem::Full),
            _ if s.starts_with("BODY.PEEK[") => {
                let section = s.strip_prefix("BODY.PEEK[")?.strip_suffix(']')?.to_string();
                Some(FetchItem::BodyPeek {
                    section,
                    partial: None,
                })
            }
            _ if s.starts_with("BODY[") => {
                let section = s.strip_prefix("BODY[")?.strip_suffix(']')?.to_string();
                Some(FetchItem::BodySection {
                    section,
                    partial: None,
                })
            }
            _ => None,
        }
    }

    /// Parse fetch items from a parenthesized list or single item
    pub fn parse_list(s: &str) -> Vec<Self> {
        let s = s.trim();

        // Single item
        if !s.starts_with('(') {
            return Self::parse(s).into_iter().collect();
        }

        // List in parentheses
        let content = s
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(s);

        // Split by whitespace, being careful about brackets
        let mut items = Vec::new();
        let mut current = String::new();
        let mut bracket_depth = 0;

        for c in content.chars() {
            match c {
                '[' => {
                    bracket_depth += 1;
                    current.push(c);
                }
                ']' => {
                    bracket_depth -= 1;
                    current.push(c);
                }
                ' ' if bracket_depth == 0 => {
                    if !current.is_empty() {
                        if let Some(item) = Self::parse(&current) {
                            items.push(item);
                        }
                        current.clear();
                    }
                }
                _ => current.push(c),
            }
        }

        if !current.is_empty() {
            if let Some(item) = Self::parse(&current) {
                items.push(item);
            }
        }

        items
    }
}

/// Search criteria
#[derive(Debug, Clone, PartialEq)]
pub enum SearchCriteria {
    /// All messages
    All,
    /// Answered messages
    Answered,
    /// BCC header contains string
    Bcc(String),
    /// Before date
    Before(String),
    /// Body contains string
    Body(String),
    /// CC header contains string
    Cc(String),
    /// Deleted messages
    Deleted,
    /// Draft messages
    Draft,
    /// Flagged messages
    Flagged,
    /// From header contains string
    From(String),
    /// Messages with specific header value
    Header(String, String),
    /// Larger than size
    Larger(u32),
    /// New messages (Recent and not Seen)
    New,
    /// Logical NOT
    Not(Box<SearchCriteria>),
    /// Old messages (not Recent)
    Old,
    /// On specific date
    On(String),
    /// Logical OR
    Or(Box<SearchCriteria>, Box<SearchCriteria>),
    /// Recent messages
    Recent,
    /// Seen messages
    Seen,
    /// Since date
    Since(String),
    /// Smaller than size
    Smaller(u32),
    /// Subject contains string
    Subject(String),
    /// Text (headers + body) contains string
    Text(String),
    /// To header contains string
    To(String),
    /// Messages with UID in set
    Uid(SequenceSet),
    /// Unanswered messages
    Unanswered,
    /// Undeleted messages
    Undeleted,
    /// Undraft messages
    Undraft,
    /// Unflagged messages
    Unflagged,
    /// Unseen messages
    Unseen,
    /// Sequence set
    SequenceSet(SequenceSet),
    /// Logical AND (multiple criteria)
    And(Vec<SearchCriteria>),
}

/// Store operation type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreOperation {
    /// Replace flags
    Replace,
    /// Add flags
    Add,
    /// Remove flags
    Remove,
}

/// Store flags specification
#[derive(Debug, Clone)]
pub struct StoreFlags {
    pub operation: StoreOperation,
    pub silent: bool,
    pub flags: Vec<String>,
}

/// IMAP Command
#[derive(Debug, Clone)]
pub enum ImapCommand {
    // Any state commands
    Capability,
    Noop,
    Logout,
    StartTls,

    // Not authenticated state
    Login {
        username: String,
        password: String,
    },
    Authenticate {
        mechanism: String,
        initial_response: Option<String>,
    },

    // Authenticated state
    Select {
        mailbox: String,
    },
    Examine {
        mailbox: String,
    },
    Create {
        mailbox: String,
    },
    Delete {
        mailbox: String,
    },
    Rename {
        old_mailbox: String,
        new_mailbox: String,
    },
    Subscribe {
        mailbox: String,
    },
    Unsubscribe {
        mailbox: String,
    },
    List {
        reference: String,
        pattern: String,
    },
    Lsub {
        reference: String,
        pattern: String,
    },
    Status {
        mailbox: String,
        items: Vec<String>,
    },
    Append {
        mailbox: String,
        flags: Vec<String>,
        date: Option<String>,
        message: Vec<u8>,
    },
    Close,

    // Selected state
    Check,
    Fetch {
        sequence: SequenceSet,
        items: Vec<FetchItem>,
        uid: bool,
    },
    Search {
        criteria: SearchCriteria,
        uid: bool,
    },
    Store {
        sequence: SequenceSet,
        flags: StoreFlags,
        uid: bool,
    },
    Copy {
        sequence: SequenceSet,
        mailbox: String,
        uid: bool,
    },
    Move {
        sequence: SequenceSet,
        mailbox: String,
        uid: bool,
    },
    Expunge,

    // Extensions
    Idle,
    Done,
    Namespace,

    // UID variants are handled via uid flag in Fetch/Search/Store/Copy/Move

    // Unknown command
    Unknown {
        command: String,
    },
}

/// Parsed IMAP command with tag
#[derive(Debug, Clone)]
pub struct TaggedCommand {
    pub tag: Tag,
    pub command: ImapCommand,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_set_parse() {
        assert_eq!(SequenceSet::parse("1"), Some(SequenceSet::Single(1)));
        assert_eq!(SequenceSet::parse("*"), Some(SequenceSet::All));
        assert_eq!(SequenceSet::parse("1:5"), Some(SequenceSet::Range(1, 5)));
        assert_eq!(
            SequenceSet::parse("1:*"),
            Some(SequenceSet::Range(1, u32::MAX))
        );
    }

    #[test]
    fn test_sequence_set_contains() {
        let set = SequenceSet::Range(1, 5);
        assert!(set.contains(1, 10));
        assert!(set.contains(3, 10));
        assert!(set.contains(5, 10));
        assert!(!set.contains(6, 10));
    }

    #[test]
    fn test_fetch_item_parse() {
        assert_eq!(FetchItem::parse("FLAGS"), Some(FetchItem::Flags));
        assert_eq!(FetchItem::parse("UID"), Some(FetchItem::Uid));
        assert_eq!(FetchItem::parse("ALL"), Some(FetchItem::All));
    }

    #[test]
    fn test_fetch_item_list() {
        let items = FetchItem::parse_list("(FLAGS UID RFC822.SIZE)");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], FetchItem::Flags);
        assert_eq!(items[1], FetchItem::Uid);
        assert_eq!(items[2], FetchItem::Rfc822Size);
    }
}
