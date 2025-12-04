# MaiRust - Claude Code Development Guide

## Project Overview

MaiRust is an open-source mail server written in Rust, designed for extensibility via webhooks.

## Development Logging

All significant implementation work should be documented in `development_logs/` with dated files.

### Log File Format
- Filename: `YYYY-MM-DD-<topic>.md`
- Example: `2024-12-04-email-auth.md`

### Log Content Structure
```markdown
# [Topic] Implementation Report

## Date
YYYY-MM-DD

## Summary
Brief description of what was implemented.

## Changes
- List of files modified/created
- Key functionality added

## Technical Details
Implementation specifics, design decisions, etc.

## Test Results
Test pass/fail status

## Next Steps (if applicable)
Future work or known issues
```

## Project Structure

```
MaiRust/
├── crates/
│   ├── mairust-common/    # Shared types and config
│   ├── mairust-storage/   # Database and file storage
│   ├── mairust-core/      # SMTP server and mail processing
│   └── mairust-api/       # REST API server
├── docs/
│   └── decisions/         # Design documents
├── development_logs/      # Development reports
└── config/                # Configuration files
```

## Key Commands

```bash
# Check compilation
cargo check

# Run tests
cargo test

# Build release
cargo build --release

# Run the server
cargo run --bin mairust
```

## Code Style

- Follow Rust standard formatting (rustfmt)
- Use meaningful variable and function names
- Add documentation comments for public APIs
- Write tests for new functionality
