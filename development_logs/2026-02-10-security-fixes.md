# Security Vulnerability Fixes Implementation Report

## Date
2026-02-10

## Summary
Implemented fixes for critical and high-severity security vulnerabilities identified in the 2026-02-09 security review reports. Addressed path traversal, tenant isolation, authentication bypass, SSRF, IMAP authorization, SMTP auth hardening, ReDoS prevention, and webhook signature verification.

## Changes

### CRITICAL Fixes

1. **Path Traversal Prevention** (`crates/mairust-storage/src/file.rs`)
   - `full_path()` now validates paths against `..` traversal, absolute paths, and canonicalizes resolved paths to ensure they stay within the storage base directory
   - Added test `test_path_traversal_prevention` to verify protection

2. **Tenant Isolation - Domain Repository** (`crates/mairust-storage/src/repository/domains.rs`)
   - Added `find_by_name_for_tenant(tenant_id, name)` method for tenant-scoped lookups
   - API handlers updated to use tenant-scoped variant where tenant_id is available

3. **Tenant Isolation - Mailbox Repository** (`crates/mairust-storage/src/repository/mailboxes.rs`)
   - Added `find_by_address_for_tenant(tenant_id, address)` method for tenant-scoped lookups
   - API handlers (mailboxes.rs, send.rs) updated to use tenant-scoped variant

4. **Authentication on Tenant API** (`crates/mairust-api/src/handlers/tenants.rs`)
   - All 4 endpoints (list, get, create, delete) now require authentication
   - List/create/delete require `admin:tenants` scope
   - Get/delete require tenant access verification

5. **Authentication on User API** (`crates/mairust-api/src/handlers/users.rs`)
   - All 4 endpoints now require authentication via `AuthContext`
   - All operations scoped to tenant via `require_tenant_access`
   - Get/delete verify user belongs to tenant before operating

6. **SSRF Prevention** (`crates/mairust-core/src/hooks/manager.rs`)
   - Added `validate_webhook_url()` to reject private/internal IPs, loopback, link-local, and non-HTTP schemes
   - Blocks cloud metadata endpoints (169.254.169.254)
   - Added `is_private_ip()` helper for IPv4/IPv6 private range detection

7. **IMAP COPY/MOVE Authorization** (`crates/mairust-core/src/imap/server.rs`)
   - COPY, MOVE, and APPEND commands now filter destination mailbox by both `tenant_id` AND `user_id`
   - Prevents cross-user mailbox access within the same tenant

8. **IMAP APPEND Size Limit** (`crates/mairust-core/src/imap/server.rs`)
   - Added `MAX_APPEND_SIZE` constant (50MB, matching SMTP limit)
   - APPEND command rejects messages exceeding the size limit

### HIGH Fixes

9. **SMTP AUTH Plaintext Fallback Removed** (`crates/mairust-core/src/smtp/auth.rs`)
   - AUTH LOGIN no longer falls back to plaintext when base64 decoding fails
   - Returns `AuthResult::failure("Invalid credentials encoding")` on decode error

10. **ReDoS Prevention** (`crates/mairust-core/src/policy/engine.rs`)
    - Both `evaluate_string_condition` and `evaluate_list_condition` regex operators now use `RegexBuilder` with a 1MB compiled size limit
    - Prevents catastrophic backtracking from malicious regex patterns

11. **API Key Prefix Query Hardening** (`crates/mairust-storage/src/repository/api_keys.rs`)
    - `find_by_prefix()` now filters out expired keys at the DB level
    - Added `LIMIT 10` to bound result set size

12. **Webhook HMAC Signatures** (`crates/mairust-core/src/hooks/manager.rs`)
    - Webhook requests now include `X-Webhook-Signature: sha256=<hex>` header when plugin has a `webhook_secret` configured
    - Uses HMAC-SHA256 for payload integrity verification
    - Added `webhook_secret` field to Plugin model

## Technical Details

### Design Decisions
- SMTP handler domain/mailbox lookups remain cross-tenant (inherent to mail routing)
- API-level lookups use tenant-scoped queries
- IMAP commands enforce user_id in addition to tenant_id for mailbox access
- Webhook HMAC signing is opt-in (only when `webhook_secret` is set on the plugin)

## Test Results
- All 88 existing tests pass
- New test `test_path_traversal_prevention` passes
- Build compiles cleanly (only pre-existing warnings)

## Next Steps (remaining from security review)
- SMTP authentication rate limiting
- STARTTLS integration tests (IMAP/POP3)
- API key hash migration to Argon2
- DKIM private key encrypted storage
- Quota enforcement for disk exhaustion prevention
- Security headers for API responses
