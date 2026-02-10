//! POP3 Command definitions
//!
//! Defines the POP3 commands supported by this server.

/// POP3 Command
#[derive(Debug, Clone)]
pub enum Pop3Command {
    // Authorization state commands
    /// USER username - Identify user
    User {
        username: String,
    },
    /// PASS password - Provide password
    Pass {
        password: String,
    },
    /// APOP name digest - Alternative authentication (MD5)
    Apop {
        name: String,
        digest: String,
    },

    // Transaction state commands
    /// STAT - Get mailbox status
    Stat,
    /// LIST [msg] - List messages
    List {
        msg: Option<u32>,
    },
    /// RETR msg - Retrieve message
    Retr {
        msg: u32,
    },
    /// DELE msg - Mark message for deletion
    Dele {
        msg: u32,
    },
    /// NOOP - No operation
    Noop,
    /// RSET - Reset (unmark all deletions)
    Rset,
    /// TOP msg n - Get message headers and first n lines
    Top {
        msg: u32,
        lines: u32,
    },
    /// UIDL [msg] - Get unique ID listing
    Uidl {
        msg: Option<u32>,
    },

    // Any state commands
    /// QUIT - End session
    Quit,
    /// CAPA - Get server capabilities
    Capa,
    /// STLS - Switch to TLS mode
    Stls,

    // Unknown command
    Unknown {
        command: String,
    },
}

/// POP3 Command Parser
pub struct Pop3Parser;

impl Pop3Parser {
    /// Parse a POP3 command line
    pub fn parse(line: &str) -> Pop3Command {
        let line = line.trim();
        if line.is_empty() {
            return Pop3Command::Unknown {
                command: String::new(),
            };
        }

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        let cmd = parts[0].to_uppercase();
        let args = if parts.len() > 1 { parts[1].trim() } else { "" };

        match cmd.as_str() {
            "USER" => Pop3Command::User {
                username: args.to_string(),
            },
            "PASS" => Pop3Command::Pass {
                password: args.to_string(),
            },
            "APOP" => {
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    Pop3Command::Apop {
                        name: parts[0].to_string(),
                        digest: parts[1].to_string(),
                    }
                } else {
                    Pop3Command::Unknown { command: cmd }
                }
            }
            "STAT" => Pop3Command::Stat,
            "LIST" => {
                let msg = if args.is_empty() {
                    None
                } else {
                    args.parse().ok()
                };
                Pop3Command::List { msg }
            }
            "RETR" => {
                if let Ok(msg) = args.parse() {
                    Pop3Command::Retr { msg }
                } else {
                    Pop3Command::Unknown { command: cmd }
                }
            }
            "DELE" => {
                if let Ok(msg) = args.parse() {
                    Pop3Command::Dele { msg }
                } else {
                    Pop3Command::Unknown { command: cmd }
                }
            }
            "NOOP" => Pop3Command::Noop,
            "RSET" => Pop3Command::Rset,
            "TOP" => {
                let parts: Vec<&str> = args.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let (Ok(msg), Ok(lines)) = (parts[0].parse(), parts[1].parse()) {
                        Pop3Command::Top { msg, lines }
                    } else {
                        Pop3Command::Unknown { command: cmd }
                    }
                } else {
                    Pop3Command::Unknown { command: cmd }
                }
            }
            "UIDL" => {
                let msg = if args.is_empty() {
                    None
                } else {
                    args.parse().ok()
                };
                Pop3Command::Uidl { msg }
            }
            "QUIT" => Pop3Command::Quit,
            "CAPA" => Pop3Command::Capa,
            "STLS" => Pop3Command::Stls,
            _ => Pop3Command::Unknown { command: cmd },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user() {
        match Pop3Parser::parse("USER testuser") {
            Pop3Command::User { username } => assert_eq!(username, "testuser"),
            _ => panic!("Expected USER command"),
        }
    }

    #[test]
    fn test_parse_pass() {
        match Pop3Parser::parse("PASS secret") {
            Pop3Command::Pass { password } => assert_eq!(password, "secret"),
            _ => panic!("Expected PASS command"),
        }
    }

    #[test]
    fn test_parse_stat() {
        assert!(matches!(Pop3Parser::parse("STAT"), Pop3Command::Stat));
    }

    #[test]
    fn test_parse_list() {
        match Pop3Parser::parse("LIST") {
            Pop3Command::List { msg } => assert!(msg.is_none()),
            _ => panic!("Expected LIST command"),
        }

        match Pop3Parser::parse("LIST 1") {
            Pop3Command::List { msg } => assert_eq!(msg, Some(1)),
            _ => panic!("Expected LIST command"),
        }
    }

    #[test]
    fn test_parse_retr() {
        match Pop3Parser::parse("RETR 1") {
            Pop3Command::Retr { msg } => assert_eq!(msg, 1),
            _ => panic!("Expected RETR command"),
        }
    }

    #[test]
    fn test_parse_dele() {
        match Pop3Parser::parse("DELE 1") {
            Pop3Command::Dele { msg } => assert_eq!(msg, 1),
            _ => panic!("Expected DELE command"),
        }
    }

    #[test]
    fn test_parse_top() {
        match Pop3Parser::parse("TOP 1 10") {
            Pop3Command::Top { msg, lines } => {
                assert_eq!(msg, 1);
                assert_eq!(lines, 10);
            }
            _ => panic!("Expected TOP command"),
        }
    }

    #[test]
    fn test_parse_uidl() {
        match Pop3Parser::parse("UIDL") {
            Pop3Command::Uidl { msg } => assert!(msg.is_none()),
            _ => panic!("Expected UIDL command"),
        }
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    #[test]
    fn test_parse_stls() {
        assert!(matches!(Pop3Parser::parse("STLS"), Pop3Command::Stls));
    }
}
