# Phase 3 Implementation Fixes Report

## Date
2025-12-05

## Summary
Implemented fixes based on Phase 3 review findings to wire all services into the main application entry point, add missing configurations, and complete partially implemented features.

## Changes

### New Files Created
- `crates/mairust-server/Cargo.toml` - New binary crate for server entry point
- `crates/mairust-server/src/main.rs` - Main application that starts all services

### Files Modified

#### Configuration (`crates/mairust-common/src/config.rs`)
- Added `Pop3Config` struct with fields: enabled, bind, starttls, timeout_minutes, max_connections
- Added `WebConfig` struct with fields: enabled, bind, api_url, debug
- Added `PluginConfig` struct with fields: enabled, plugin_dir, timeout_ms, enable_categorizer, ai_endpoint
- Added default functions for all new config options

#### IMAP Module (`crates/mairust-core/src/imap/mod.rs`)
- Updated documentation comments to reflect full IMAP4rev1 support (read/write operations)
- Listed all supported commands including STORE, COPY, MOVE, EXPUNGE, APPEND

#### Plugin Manager (`crates/mairust-core/src/plugins/manager.rs`)
- Added `PluginManifest` struct for parsing plugin.toml files
- Implemented `load_plugins_from_directory()` to scan and load plugins from configured plugin_dir
- Implemented `load_plugin_from_manifest()` to parse individual plugin manifests
- Implemented full `install_plugin()` workflow: validate path, parse manifest, check compatibility, copy to plugin directory, register plugin
- Fixed PluginInfo creation to use correct fields (homepage, capabilities, protocol)
- Added PluginProtocol import

#### Web UI Handlers (`crates/mairust-web/src/handlers.rs`)
- Implemented session-based authentication using cookies
- Added `check_auth()` function to validate session from database
- Updated all page handlers (inbox, compose, message, settings) to require authentication
- Implemented proper `login_submit()` with:
  - User lookup by email
  - Argon2 password verification
  - Session creation in database
  - Secure cookie setting
- Implemented `logout()` with session cleanup

#### Web UI Dependencies (`crates/mairust-web/Cargo.toml`)
- Added axum-extra for cookie support
- Added time crate for duration handling
- Added argon2 for password verification
- Added sqlx for database queries

#### Workspace (`Cargo.toml`)
- Added mairust-server to workspace members
- Added axum-extra dependency
- Added time crate dependency

#### Core Crate (`crates/mairust-core/Cargo.toml`)
- Removed binary target (moved to mairust-server)
- Removed mairust-api and mairust-web dependencies to avoid circular dependency
- Removed axum dependency (not needed in library)
- Added toml crate for plugin manifest parsing

### Main Application (`crates/mairust-server/src/main.rs`)
Implemented full server startup with all services:
- SMTP server (always on)
- IMAP server (if enabled)
- POP3 server (if enabled)
- API server (always on)
- Web UI server (if enabled)
- Plugin manager initialization
- Queue manager and processor
- Graceful shutdown handling

## Technical Details

### Architecture Change
Split the main binary from mairust-core into a separate mairust-server crate to resolve circular dependency:
- mairust-core -> mairust-storage (OK)
- mairust-api -> mairust-core (OK for search functionality)
- mairust-server -> mairust-core, mairust-api, mairust-web (OK)

This allows each component to be developed independently while maintaining the ability to run all services from a single binary.

### Service Configuration
All optional services (IMAP, POP3, Web UI) are controlled by `enabled` flags in their respective config sections:
```toml
[imap]
enabled = true
bind = "0.0.0.0:143"

[pop3]
enabled = true
bind = "0.0.0.0:110"

[web]
enabled = true
bind = "0.0.0.0:8081"

[plugins]
enabled = true
plugin_dir = "/var/lib/mairust/plugins"
```

### Plugin System
Plugins are loaded from TOML manifest files with format:
```toml
id = "my-plugin"
name = "My Plugin"
version = "1.0.0"
description = "Plugin description"
author = "Author Name"
plugin_type = "categorization"
endpoint = "http://localhost:8080/webhook"
```

## Test Results
All 88 tests passed:
- mairust_common: 5 tests
- mairust_core: 82 tests
- mairust_storage: 1 test

## Next Steps
- Add API endpoints for threading and tagging
- Implement message pipeline integration for threading/tagging
- Add frontend JavaScript for API data fetching in Web UI
- Add end-to-end integration tests
