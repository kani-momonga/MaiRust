# IMAP/POP3 Message Storage Fix Implementation Report

## Date
2025-12-05

## Summary
Fixed critical P1 issues where IMAP APPEND was truncating message content to 500 characters and POP3 RETR was returning previews instead of full messages. The fix ensures that full message content is properly stored to file storage and retrieved when requested.

## Issues Addressed
1. **IMAP APPEND truncation (P1)**: APPEND command only kept a 500-character preview and never persisted the full message bytes to storage_path
2. **POP3 RETR returns previews (P1)**: RETR command returned only the truncated preview instead of the full message from storage

## Changes

### Files Modified

#### `crates/mairust-core/src/imap/server.rs`
- Added imports for `FileStorage`, `LocalStorage`, and `PathBuf`
- Added `storage_path: PathBuf` field to `ImapConfig` struct
- Updated `Default` implementation for `ImapConfig` to include `storage_path`
- Modified `handle_command` to accept `storage_path` parameter
- Modified `handle_fetch` to:
  - Accept `storage_path` parameter
  - Read full message body from file storage for BodySection/BodyPeek requests
  - Fall back to body_preview if storage read fails
- Modified `handle_append` to:
  - Accept `storage_path` parameter
  - Initialize LocalStorage and write full message to file storage
  - Return error if storage operation fails
  - Still create truncated body_preview for quick display purposes

#### `crates/mairust-core/src/pop3/server.rs`
- Added imports for `FileStorage`, `LocalStorage`, and `PathBuf`
- Added `storage_path: PathBuf` field to `Pop3Config` struct
- Updated `Default` implementation for `Pop3Config` to include `storage_path`
- Modified `handle_command` to accept `storage_path` parameter
- Modified `handle_retr` to:
  - Accept `storage_path` parameter instead of `db_pool`
  - Read full message from file storage using `storage_path`
  - Return actual message size (not preview size) in response header
  - Fall back to body_preview if storage read fails
- Modified `handle_top` to:
  - Accept `storage_path` parameter instead of `db_pool`
  - Read full message from file storage
  - Properly parse headers and body, returning headers plus N lines of body
  - Fall back to body_preview if storage read fails

## Technical Details

### Architecture
The fix follows the existing storage pattern in MaiRust:
- `LocalStorage` from `mairust-storage` provides file operations (store, read, delete)
- Storage path is configured via `storage_path` field in config structs
- Messages are stored at path: `{tenant_id}/{mailbox_id}/{message_id}.eml`

### Backward Compatibility
- Existing messages without file storage will fall back to body_preview
- New messages are always stored to file storage
- Preview functionality retained for quick message listing

### Error Handling
- Storage initialization failures return appropriate IMAP/POP3 error responses
- File read failures fall back gracefully to body_preview with warning logs
- Database operations are only performed after successful file storage

## Test Results
All 83 existing tests pass:
- `cargo check`: Successful (with existing unrelated warnings)
- `cargo test`: 83 passed, 0 failed

## Next Steps (if applicable)
- Consider adding integration tests for full message round-trip
- Monitor storage usage and implement cleanup for deleted messages
- Consider adding message compression for large attachments
