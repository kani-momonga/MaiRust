//! IMAP Response generation
//!
//! Generates IMAP4 response strings for client communication.

use chrono::{DateTime, Utc};

/// IMAP response status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseStatus {
    Ok,
    No,
    Bad,
    Bye,
    Preauth,
}

impl std::fmt::Display for ResponseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseStatus::Ok => write!(f, "OK"),
            ResponseStatus::No => write!(f, "NO"),
            ResponseStatus::Bad => write!(f, "BAD"),
            ResponseStatus::Bye => write!(f, "BYE"),
            ResponseStatus::Preauth => write!(f, "PREAUTH"),
        }
    }
}

/// IMAP Response builder
pub struct ImapResponse;

impl ImapResponse {
    /// Server greeting
    pub fn greeting() -> String {
        "* OK [CAPABILITY IMAP4rev1 LITERAL+ SASL-IR LOGIN AUTH=PLAIN] MaiRust IMAP server ready\r\n".to_string()
    }

    /// Tagged OK response
    pub fn ok(tag: &str, message: &str) -> String {
        format!("{} OK {}\r\n", tag, message)
    }

    /// Tagged NO response
    pub fn no(tag: &str, message: &str) -> String {
        format!("{} NO {}\r\n", tag, message)
    }

    /// Tagged BAD response
    pub fn bad(tag: &str, message: &str) -> String {
        format!("{} BAD {}\r\n", tag, message)
    }

    /// Untagged BYE response
    pub fn bye(message: &str) -> String {
        format!("* BYE {}\r\n", message)
    }

    /// CAPABILITY response
    pub fn capability() -> String {
        "* CAPABILITY IMAP4rev1 LITERAL+ SASL-IR LOGIN AUTH=PLAIN IDLE NAMESPACE MOVE UIDPLUS\r\n".to_string()
    }

    /// Update CAPABILITY in greeting to include write operations
    pub fn greeting_full() -> String {
        "* OK [CAPABILITY IMAP4rev1 LITERAL+ SASL-IR LOGIN AUTH=PLAIN IDLE MOVE UIDPLUS] MaiRust IMAP server ready\r\n".to_string()
    }

    /// EXPUNGE response
    pub fn expunge(seq: u32) -> String {
        format!("* {} EXPUNGE\r\n", seq)
    }

    /// COPYUID/APPENDUID response code
    pub fn copyuid(uid_validity: u32, source_uids: &str, dest_uids: &str) -> String {
        format!("[COPYUID {} {} {}]", uid_validity, source_uids, dest_uids)
    }

    pub fn appenduid(uid_validity: u32, uid: u32) -> String {
        format!("[APPENDUID {} {}]", uid_validity, uid)
    }

    /// NAMESPACE response
    pub fn namespace() -> String {
        // Personal namespace, Other users namespace, Shared namespace
        "* NAMESPACE ((\"\" \"/\")) NIL NIL\r\n".to_string()
    }

    /// LIST response for a mailbox
    pub fn list(flags: &[&str], delimiter: &str, mailbox: &str) -> String {
        let flags_str = flags.join(" ");
        format!(
            "* LIST ({}) \"{}\" \"{}\"\r\n",
            flags_str, delimiter, mailbox
        )
    }

    /// LSUB response for a mailbox
    pub fn lsub(flags: &[&str], delimiter: &str, mailbox: &str) -> String {
        let flags_str = flags.join(" ");
        format!(
            "* LSUB ({}) \"{}\" \"{}\"\r\n",
            flags_str, delimiter, mailbox
        )
    }

    /// SELECT/EXAMINE response components
    pub fn mailbox_flags(flags: &[&str]) -> String {
        let flags_str = flags.join(" ");
        format!("* FLAGS ({})\r\n", flags_str)
    }

    pub fn permanent_flags(flags: &[&str]) -> String {
        let flags_str = flags.join(" ");
        format!("* OK [PERMANENTFLAGS ({})] Flags permitted\r\n", flags_str)
    }

    pub fn exists(count: u32) -> String {
        format!("* {} EXISTS\r\n", count)
    }

    pub fn recent(count: u32) -> String {
        format!("* {} RECENT\r\n", count)
    }

    pub fn unseen(first_unseen: u32) -> String {
        format!("* OK [UNSEEN {}] First unseen\r\n", first_unseen)
    }

    pub fn uid_validity(validity: u32) -> String {
        format!("* OK [UIDVALIDITY {}] UIDs valid\r\n", validity)
    }

    pub fn uid_next(next: u32) -> String {
        format!("* OK [UIDNEXT {}] Predicted next UID\r\n", next)
    }

    /// STATUS response
    pub fn status(mailbox: &str, items: &[(String, u32)]) -> String {
        let items_str: Vec<String> = items.iter().map(|(k, v)| format!("{} {}", k, v)).collect();
        format!("* STATUS \"{}\" ({})\r\n", mailbox, items_str.join(" "))
    }

    /// FETCH response
    pub fn fetch(seq: u32, items: &[(String, String)]) -> String {
        let items_str: Vec<String> = items.iter().map(|(k, v)| format!("{} {}", k, v)).collect();
        format!("* {} FETCH ({})\r\n", seq, items_str.join(" "))
    }

    /// FETCH with literal body
    pub fn fetch_with_body(seq: u32, items: &[(String, String)], body_key: &str, body: &str) -> String {
        let items_str: Vec<String> = items.iter().map(|(k, v)| format!("{} {}", k, v)).collect();
        let literal_len = body.len();
        let mut parts = items_str;
        parts.push(format!("{} {{{}}}\r\n{}", body_key, literal_len, body));
        format!("* {} FETCH ({})\r\n", seq, parts.join(" "))
    }

    /// SEARCH response
    pub fn search(uids: &[u32]) -> String {
        if uids.is_empty() {
            "* SEARCH\r\n".to_string()
        } else {
            let uids_str: Vec<String> = uids.iter().map(|u| u.to_string()).collect();
            format!("* SEARCH {}\r\n", uids_str.join(" "))
        }
    }

    /// Format flags for FETCH
    pub fn format_flags(seen: bool, answered: bool, flagged: bool, deleted: bool, draft: bool) -> String {
        let mut flags = Vec::new();
        if seen {
            flags.push("\\Seen");
        }
        if answered {
            flags.push("\\Answered");
        }
        if flagged {
            flags.push("\\Flagged");
        }
        if deleted {
            flags.push("\\Deleted");
        }
        if draft {
            flags.push("\\Draft");
        }
        format!("({})", flags.join(" "))
    }

    /// Format internal date for FETCH
    pub fn format_internal_date(dt: &DateTime<Utc>) -> String {
        format!("\"{}\"", dt.format("%d-%b-%Y %H:%M:%S %z"))
    }

    /// Format envelope for FETCH
    pub fn format_envelope(
        date: Option<&str>,
        subject: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        cc: Option<&str>,
        message_id: Option<&str>,
    ) -> String {
        let date_str = date.map(|s| format!("\"{}\"", s)).unwrap_or_else(|| "NIL".to_string());
        let subject_str = subject.map(|s| format!("\"{}\"", Self::quote_string(s))).unwrap_or_else(|| "NIL".to_string());
        let from_str = Self::format_address_list(from);
        let to_str = Self::format_address_list(to);
        let cc_str = Self::format_address_list(cc);
        let message_id_str = message_id.map(|s| format!("\"{}\"", s)).unwrap_or_else(|| "NIL".to_string());

        // Full envelope: (date subject from sender reply-to to cc bcc in-reply-to message-id)
        format!(
            "({} {} {} {} {} {} {} NIL NIL {})",
            date_str,
            subject_str,
            from_str,
            from_str, // sender = from
            from_str, // reply-to = from
            to_str,
            cc_str,
            message_id_str
        )
    }

    /// Format address list for envelope
    fn format_address_list(addr: Option<&str>) -> String {
        match addr {
            None => "NIL".to_string(),
            Some(addr) => {
                // Parse simple email address
                if let Some((name, email)) = Self::parse_address(addr) {
                    let (local, domain) = if let Some(pos) = email.find('@') {
                        (email[..pos].to_string(), email[pos + 1..].to_string())
                    } else {
                        (email.clone(), String::new())
                    };

                    format!(
                        "((\"{}\" NIL \"{}\" \"{}\"))",
                        Self::quote_string(&name),
                        Self::quote_string(&local),
                        Self::quote_string(&domain)
                    )
                } else {
                    "NIL".to_string()
                }
            }
        }
    }

    /// Parse email address into name and email parts
    fn parse_address(addr: &str) -> Option<(String, String)> {
        // Handle "Name <email>" format
        if let Some(start) = addr.find('<') {
            if let Some(end) = addr.find('>') {
                let name = addr[..start].trim().trim_matches('"').to_string();
                let email = addr[start + 1..end].to_string();
                return Some((name, email));
            }
        }

        // Just email address
        if addr.contains('@') {
            return Some((String::new(), addr.to_string()));
        }

        None
    }

    /// Quote a string for IMAP (escape backslash and quote)
    fn quote_string(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Format BODYSTRUCTURE for a simple text/plain message
    pub fn format_body_structure_simple(size: u64, lines: u32) -> String {
        format!(
            "(\"TEXT\" \"PLAIN\" (\"CHARSET\" \"UTF-8\") NIL NIL \"7BIT\" {} {} NIL NIL NIL NIL)",
            size, lines
        )
    }

    /// Continue response for AUTHENTICATE
    pub fn continue_req() -> String {
        "+ \r\n".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting() {
        let greeting = ImapResponse::greeting();
        assert!(greeting.starts_with("* OK"));
        assert!(greeting.contains("IMAP4rev1"));
    }

    #[test]
    fn test_ok() {
        assert_eq!(ImapResponse::ok("A001", "Success"), "A001 OK Success\r\n");
    }

    #[test]
    fn test_no() {
        assert_eq!(ImapResponse::no("A001", "Failed"), "A001 NO Failed\r\n");
    }

    #[test]
    fn test_format_flags() {
        let flags = ImapResponse::format_flags(true, false, true, false, false);
        assert_eq!(flags, "(\\Seen \\Flagged)");
    }

    #[test]
    fn test_search() {
        assert_eq!(ImapResponse::search(&[1, 2, 5]), "* SEARCH 1 2 5\r\n");
        assert_eq!(ImapResponse::search(&[]), "* SEARCH\r\n");
    }

    #[test]
    fn test_list() {
        let list = ImapResponse::list(&["\\HasNoChildren"], "/", "INBOX");
        assert_eq!(list, "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n");
    }
}
