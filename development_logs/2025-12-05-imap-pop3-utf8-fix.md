# IMAP/POP3 UTF-8 Conversion Fix Implementation Report

## Date
2025-12-05

## Summary
Fixed P1 issues in IMAP FETCH Body[] and POP3 RETR commands where UTF-8 lossy conversion caused mismatch between advertised byte lengths and actual payload sizes.

## Changes
- `crates/mairust-core/src/imap/server.rs` (line 753)
  - Fixed FETCH BodySection/BodyPeek to use converted string's byte length instead of raw data length
- `crates/mairust-core/src/pop3/server.rs` (lines 451-486)
  - Fixed RETR command to build body content first, then calculate accurate size for header

## Technical Details

### Issue 1: IMAP FETCH Body[] Literal Length Mismatch

**Problem:** The code read message bytes and converted them to UTF-8 using `String::from_utf8_lossy()`, but advertised the original raw byte length in the IMAP literal `{length}` format. When non-UTF-8 bytes (e.g., binary parts or 8-bit bodies) were replaced with the Unicode replacement character (U+FFFD, which is 3 bytes in UTF-8), the actual transmitted bytes differed from the advertised length.

**Before:**
```rust
let body = String::from_utf8_lossy(&data);
fetch_items.push((body_key, format!("{{{}}}\r\n{}", data.len(), body)));
```

**After:**
```rust
let body = String::from_utf8_lossy(&data);
fetch_items.push((body_key, format!("{{{}}}\r\n{}", body.len(), body)));
```

### Issue 2: POP3 RETR Octet Count Inconsistency

**Problem:** The RETR command reported the original message byte size in the `+OK` header, but the actual body underwent multiple transformations:
1. UTF-8 lossy conversion (can change byte count)
2. Line splitting via `.lines()` (normalizes line endings)
3. Byte-stuffing (adds '.' prefix to lines starting with '.')

The final payload byte count could differ significantly from the advertised octet count.

**Before:**
```rust
let body = String::from_utf8_lossy(&message_data);
let mut response = Pop3Response::retr_header(message_data.len() as u64);
for line in body.lines() {
    response.push_str(&Pop3Response::byte_stuff_line(line));
    response.push_str("\r\n");
}
```

**After:**
```rust
let body = String::from_utf8_lossy(&message_data);
let mut body_content = String::new();
for line in body.lines() {
    body_content.push_str(&Pop3Response::byte_stuff_line(line));
    body_content.push_str("\r\n");
}
let mut response = Pop3Response::retr_header(body_content.len() as u64);
response.push_str(&body_content);
```

## Test Results
All 88 tests pass:
- mairust_common: 5 tests
- mairust_core: 82 tests
- mairust_storage: 1 test

## Impact
These fixes prevent IMAP and POP3 clients from:
- Misparsing responses due to incorrect literal lengths
- Hanging while waiting for additional bytes
- Truncating messages that had byte count mismatches
