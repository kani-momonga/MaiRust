//! IMAP Command Parser
//!
//! Parses IMAP4 commands from client input.

use super::command::{FetchItem, ImapCommand, SearchCriteria, SequenceSet, TaggedCommand};
use tracing::debug;

/// IMAP command parser
pub struct ImapParser;

impl ImapParser {
    /// Parse an IMAP command line
    pub fn parse(line: &str) -> Option<TaggedCommand> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }

        // Split into tag and command
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.is_empty() {
            return None;
        }

        let tag = parts[0].to_string();
        let rest = if parts.len() > 1 { parts[1].trim() } else { "" };

        // Parse command
        let command = Self::parse_command(rest)?;

        Some(TaggedCommand { tag, command })
    }

    /// Parse the command portion
    fn parse_command(input: &str) -> Option<ImapCommand> {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd_name = parts[0].to_uppercase();
        let args = if parts.len() > 1 { parts[1] } else { "" };

        match cmd_name.as_str() {
            // Any state
            "CAPABILITY" => Some(ImapCommand::Capability),
            "NOOP" => Some(ImapCommand::Noop),
            "LOGOUT" => Some(ImapCommand::Logout),

            // Not authenticated
            "LOGIN" => Self::parse_login(args),
            "AUTHENTICATE" => Self::parse_authenticate(args),

            // Authenticated
            "SELECT" => Some(ImapCommand::Select {
                mailbox: Self::parse_mailbox(args),
            }),
            "EXAMINE" => Some(ImapCommand::Examine {
                mailbox: Self::parse_mailbox(args),
            }),
            "LIST" => Self::parse_list(args),
            "LSUB" => Self::parse_lsub(args),
            "STATUS" => Self::parse_status(args),
            "CLOSE" => Some(ImapCommand::Close),

            // Selected
            "CHECK" => Some(ImapCommand::Check),
            "FETCH" => Self::parse_fetch(args, false),
            "SEARCH" => Self::parse_search(args, false),
            "UID" => Self::parse_uid_command(args),

            _ => Some(ImapCommand::Unknown {
                command: cmd_name,
            }),
        }
    }

    /// Parse LOGIN command arguments
    fn parse_login(args: &str) -> Option<ImapCommand> {
        let (username, rest) = Self::parse_astring(args)?;
        let (password, _) = Self::parse_astring(rest.trim())?;
        Some(ImapCommand::Login { username, password })
    }

    /// Parse AUTHENTICATE command
    fn parse_authenticate(args: &str) -> Option<ImapCommand> {
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        let mechanism = parts[0].to_uppercase();
        let initial_response = parts.get(1).map(|s| s.to_string());
        Some(ImapCommand::Authenticate {
            mechanism,
            initial_response,
        })
    }

    /// Parse LIST command
    fn parse_list(args: &str) -> Option<ImapCommand> {
        let (reference, rest) = Self::parse_astring(args)?;
        let (pattern, _) = Self::parse_astring(rest.trim())?;
        Some(ImapCommand::List { reference, pattern })
    }

    /// Parse LSUB command
    fn parse_lsub(args: &str) -> Option<ImapCommand> {
        let (reference, rest) = Self::parse_astring(args)?;
        let (pattern, _) = Self::parse_astring(rest.trim())?;
        Some(ImapCommand::Lsub { reference, pattern })
    }

    /// Parse STATUS command
    fn parse_status(args: &str) -> Option<ImapCommand> {
        // STATUS mailbox (item1 item2 ...)
        let (mailbox, rest) = Self::parse_astring(args)?;
        let rest = rest.trim();

        // Parse items list
        let items = if rest.starts_with('(') && rest.ends_with(')') {
            let content = rest.strip_prefix('(')?.strip_suffix(')')?;
            content.split_whitespace().map(|s| s.to_uppercase()).collect()
        } else {
            vec![]
        };

        Some(ImapCommand::Status { mailbox, items })
    }

    /// Parse FETCH command
    fn parse_fetch(args: &str, uid: bool) -> Option<ImapCommand> {
        // FETCH sequence items
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        if parts.is_empty() {
            return None;
        }

        let sequence = SequenceSet::parse(parts[0])?;
        let items_str = if parts.len() > 1 { parts[1] } else { "" };
        let items = FetchItem::parse_list(items_str);

        Some(ImapCommand::Fetch {
            sequence,
            items,
            uid,
        })
    }

    /// Parse SEARCH command
    fn parse_search(args: &str, uid: bool) -> Option<ImapCommand> {
        let criteria = Self::parse_search_criteria(args)?;
        Some(ImapCommand::Search { criteria, uid })
    }

    /// Parse search criteria
    fn parse_search_criteria(args: &str) -> Option<SearchCriteria> {
        let args = args.trim();
        if args.is_empty() {
            return Some(SearchCriteria::All);
        }

        let args_upper = args.to_uppercase();

        // Simple criteria
        match args_upper.as_str() {
            "ALL" => return Some(SearchCriteria::All),
            "ANSWERED" => return Some(SearchCriteria::Answered),
            "DELETED" => return Some(SearchCriteria::Deleted),
            "DRAFT" => return Some(SearchCriteria::Draft),
            "FLAGGED" => return Some(SearchCriteria::Flagged),
            "NEW" => return Some(SearchCriteria::New),
            "OLD" => return Some(SearchCriteria::Old),
            "RECENT" => return Some(SearchCriteria::Recent),
            "SEEN" => return Some(SearchCriteria::Seen),
            "UNANSWERED" => return Some(SearchCriteria::Unanswered),
            "UNDELETED" => return Some(SearchCriteria::Undeleted),
            "UNDRAFT" => return Some(SearchCriteria::Undraft),
            "UNFLAGGED" => return Some(SearchCriteria::Unflagged),
            "UNSEEN" => return Some(SearchCriteria::Unseen),
            _ => {}
        }

        // Check for criteria with arguments
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        let key = parts[0].to_uppercase();
        let value = if parts.len() > 1 { parts[1] } else { "" };

        match key.as_str() {
            "BCC" => {
                let (s, _) = Self::parse_astring(value)?;
                Some(SearchCriteria::Bcc(s))
            }
            "BEFORE" => Some(SearchCriteria::Before(value.to_string())),
            "BODY" => {
                let (s, _) = Self::parse_astring(value)?;
                Some(SearchCriteria::Body(s))
            }
            "CC" => {
                let (s, _) = Self::parse_astring(value)?;
                Some(SearchCriteria::Cc(s))
            }
            "FROM" => {
                let (s, _) = Self::parse_astring(value)?;
                Some(SearchCriteria::From(s))
            }
            "LARGER" => Some(SearchCriteria::Larger(value.parse().ok()?)),
            "ON" => Some(SearchCriteria::On(value.to_string())),
            "SINCE" => Some(SearchCriteria::Since(value.to_string())),
            "SMALLER" => Some(SearchCriteria::Smaller(value.parse().ok()?)),
            "SUBJECT" => {
                let (s, _) = Self::parse_astring(value)?;
                Some(SearchCriteria::Subject(s))
            }
            "TEXT" => {
                let (s, _) = Self::parse_astring(value)?;
                Some(SearchCriteria::Text(s))
            }
            "TO" => {
                let (s, _) = Self::parse_astring(value)?;
                Some(SearchCriteria::To(s))
            }
            "NOT" => {
                let inner = Self::parse_search_criteria(value)?;
                Some(SearchCriteria::Not(Box::new(inner)))
            }
            "UID" => {
                let seq = SequenceSet::parse(value)?;
                Some(SearchCriteria::Uid(seq))
            }
            _ => {
                // Try to parse as sequence set
                if let Some(seq) = SequenceSet::parse(&key) {
                    Some(SearchCriteria::SequenceSet(seq))
                } else {
                    Some(SearchCriteria::All)
                }
            }
        }
    }

    /// Parse UID FETCH/SEARCH commands
    fn parse_uid_command(args: &str) -> Option<ImapCommand> {
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        if parts.is_empty() {
            return None;
        }

        let subcmd = parts[0].to_uppercase();
        let subargs = if parts.len() > 1 { parts[1] } else { "" };

        match subcmd.as_str() {
            "FETCH" => Self::parse_fetch(subargs, true),
            "SEARCH" => Self::parse_search(subargs, true),
            _ => Some(ImapCommand::Unknown { command: format!("UID {}", subcmd) }),
        }
    }

    /// Parse mailbox name
    fn parse_mailbox(s: &str) -> String {
        let s = s.trim();
        if s.starts_with('"') && s.ends_with('"') {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }

    /// Parse an astring (atom or quoted string)
    /// Returns the parsed string and remaining input
    fn parse_astring(s: &str) -> Option<(String, &str)> {
        let s = s.trim();

        if s.starts_with('"') {
            // Quoted string
            let mut chars = s.chars().skip(1);
            let mut result = String::new();
            let mut escaped = false;
            let mut pos = 1;

            for c in chars {
                pos += 1;
                if escaped {
                    result.push(c);
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    break;
                } else {
                    result.push(c);
                }
            }

            Some((result, &s[pos..]))
        } else {
            // Atom (space-delimited)
            let end = s.find(' ').unwrap_or(s.len());
            Some((s[..end].to_string(), &s[end..]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_capability() {
        let cmd = ImapParser::parse("A001 CAPABILITY").unwrap();
        assert_eq!(cmd.tag, "A001");
        assert!(matches!(cmd.command, ImapCommand::Capability));
    }

    #[test]
    fn test_parse_login() {
        let cmd = ImapParser::parse("A002 LOGIN user password").unwrap();
        assert_eq!(cmd.tag, "A002");
        if let ImapCommand::Login { username, password } = cmd.command {
            assert_eq!(username, "user");
            assert_eq!(password, "password");
        } else {
            panic!("Expected LOGIN command");
        }
    }

    #[test]
    fn test_parse_login_quoted() {
        let cmd = ImapParser::parse(r#"A002 LOGIN "user@example.com" "pass word""#).unwrap();
        if let ImapCommand::Login { username, password } = cmd.command {
            assert_eq!(username, "user@example.com");
            assert_eq!(password, "pass word");
        } else {
            panic!("Expected LOGIN command");
        }
    }

    #[test]
    fn test_parse_select() {
        let cmd = ImapParser::parse("A003 SELECT INBOX").unwrap();
        if let ImapCommand::Select { mailbox } = cmd.command {
            assert_eq!(mailbox, "INBOX");
        } else {
            panic!("Expected SELECT command");
        }
    }

    #[test]
    fn test_parse_fetch() {
        let cmd = ImapParser::parse("A004 FETCH 1:* (FLAGS UID)").unwrap();
        if let ImapCommand::Fetch { sequence, items, uid } = cmd.command {
            assert!(!uid);
            assert!(matches!(sequence, SequenceSet::Range(1, _)));
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected FETCH command");
        }
    }

    #[test]
    fn test_parse_uid_fetch() {
        let cmd = ImapParser::parse("A005 UID FETCH 1:100 FLAGS").unwrap();
        if let ImapCommand::Fetch { sequence, items, uid } = cmd.command {
            assert!(uid);
            assert!(matches!(sequence, SequenceSet::Range(1, 100)));
        } else {
            panic!("Expected UID FETCH command");
        }
    }

    #[test]
    fn test_parse_search() {
        let cmd = ImapParser::parse("A006 SEARCH UNSEEN").unwrap();
        if let ImapCommand::Search { criteria, uid } = cmd.command {
            assert!(!uid);
            assert!(matches!(criteria, SearchCriteria::Unseen));
        } else {
            panic!("Expected SEARCH command");
        }
    }

    #[test]
    fn test_parse_list() {
        let cmd = ImapParser::parse(r#"A007 LIST "" "*""#).unwrap();
        if let ImapCommand::List { reference, pattern } = cmd.command {
            assert_eq!(reference, "");
            assert_eq!(pattern, "*");
        } else {
            panic!("Expected LIST command");
        }
    }
}
