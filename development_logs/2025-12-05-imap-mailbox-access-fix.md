# IMAP Mailbox Access Control Fix

## Date
2025-12-05

## Summary
Fixed a critical security vulnerability in the IMAP server where SELECT/EXAMINE and STATUS commands did not properly restrict mailbox access by user_id, allowing any authenticated user within a tenant to access another user's mailbox.

## Changes
- Modified `crates/mairust-core/src/imap/server.rs`:
  - `handle_select()`: Added `user_id` constraint to non-INBOX mailbox query (line 358)
  - `handle_status()`: Added `user_id` extraction and constraint to mailbox query (lines 485-500)

## Technical Details

### Vulnerability Description
The IMAP SELECT/EXAMINE handler had two code paths:
1. **INBOX case**: Correctly filtered by `tenant_id` AND `user_id`
2. **Non-INBOX case**: Only filtered by `tenant_id` and `address`, missing `user_id`

This meant any authenticated user in a tenant could SELECT another user's mailbox simply by providing its address, then FETCH all messages from that mailbox.

The STATUS command had the same vulnerability, allowing users to see message counts and other metadata of other users' mailboxes.

### Fix Applied

**Before (SELECT non-INBOX):**
```sql
SELECT id, address FROM mailboxes WHERE tenant_id = $1 AND address = $2
```

**After (SELECT non-INBOX):**
```sql
SELECT id, address FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND address = $3
```

**Before (STATUS):**
```sql
SELECT id FROM mailboxes WHERE tenant_id = $1 AND address = $2
```

**After (STATUS):**
```sql
SELECT id FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND address = $3
```

## Test Results
All 24 IMAP tests pass:
- Parser tests: 8 passed
- Response tests: 6 passed
- Session tests: 5 passed
- Server tests: 1 passed
- Command tests: 4 passed

## Security Impact
- **Severity**: P1 (Critical)
- **Type**: Authorization bypass / per-user isolation violation
- **CVSS**: High (allows unauthorized access to confidential email data)
- **Attack Vector**: Authenticated user within the same tenant
