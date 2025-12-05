//! POP3 Response generation
//!
//! Generates POP3 response strings for client communication.

/// POP3 Response builder
pub struct Pop3Response;

impl Pop3Response {
    /// Server greeting
    pub fn greeting(server_name: &str) -> String {
        format!("+OK {} POP3 server ready\r\n", server_name)
    }

    /// Positive response
    pub fn ok(message: &str) -> String {
        format!("+OK {}\r\n", message)
    }

    /// Positive response with no message
    pub fn ok_simple() -> String {
        "+OK\r\n".to_string()
    }

    /// Negative response
    pub fn err(message: &str) -> String {
        format!("-ERR {}\r\n", message)
    }

    /// STAT response
    pub fn stat(count: u32, size: u64) -> String {
        format!("+OK {} {}\r\n", count, size)
    }

    /// LIST response header
    pub fn list_header(count: u32, size: u64) -> String {
        format!("+OK {} messages ({} octets)\r\n", count, size)
    }

    /// LIST single message response
    pub fn list_single(msg: u32, size: u64) -> String {
        format!("+OK {} {}\r\n", msg, size)
    }

    /// LIST line for multi-line response
    pub fn list_line(msg: u32, size: u64) -> String {
        format!("{} {}\r\n", msg, size)
    }

    /// UIDL response header
    pub fn uidl_header() -> String {
        "+OK\r\n".to_string()
    }

    /// UIDL single message response
    pub fn uidl_single(msg: u32, uid: &str) -> String {
        format!("+OK {} {}\r\n", msg, uid)
    }

    /// UIDL line for multi-line response
    pub fn uidl_line(msg: u32, uid: &str) -> String {
        format!("{} {}\r\n", msg, uid)
    }

    /// RETR response header
    pub fn retr_header(size: u64) -> String {
        format!("+OK {} octets\r\n", size)
    }

    /// TOP response header
    pub fn top_header() -> String {
        "+OK\r\n".to_string()
    }

    /// CAPA response
    pub fn capabilities() -> String {
        "+OK Capability list follows\r\n\
         USER\r\n\
         TOP\r\n\
         UIDL\r\n\
         IMPLEMENTATION MaiRust-POP3\r\n\
         .\r\n"
            .to_string()
    }

    /// Multi-line terminator
    pub fn terminator() -> String {
        ".\r\n".to_string()
    }

    /// Byte-stuff a line (add leading dot if line starts with dot)
    pub fn byte_stuff_line(line: &str) -> String {
        if line.starts_with('.') {
            format!(".{}", line)
        } else {
            line.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting() {
        let greeting = Pop3Response::greeting("MaiRust");
        assert!(greeting.starts_with("+OK"));
        assert!(greeting.contains("POP3 server ready"));
    }

    #[test]
    fn test_ok() {
        assert_eq!(Pop3Response::ok("Success"), "+OK Success\r\n");
    }

    #[test]
    fn test_err() {
        assert_eq!(Pop3Response::err("Failed"), "-ERR Failed\r\n");
    }

    #[test]
    fn test_stat() {
        assert_eq!(Pop3Response::stat(5, 1000), "+OK 5 1000\r\n");
    }

    #[test]
    fn test_byte_stuffing() {
        assert_eq!(Pop3Response::byte_stuff_line(".hello"), "..hello");
        assert_eq!(Pop3Response::byte_stuff_line("hello"), "hello");
    }
}
