# MaiRust

**Modern, API-first, AI-enabled mail server written in Rust**

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

MaiRust is an open-source mail server designed for extensibility via webhooks and plugins. It provides a modern alternative to traditional mail servers with a focus on API-driven operations and AI integration capabilities.

## Features

### Core Capabilities
- **SMTP Server** - Inbound (port 25) and Submission (port 587) with STARTTLS
- **Email Authentication** - SPF validation, DKIM signing/verification, DMARC support
- **REST API** - Full API for user, domain, mailbox, and message management
- **Database Support** - PostgreSQL (recommended)
- **Flexible Storage** - Local filesystem or S3-compatible object storage

### Plugin & Hook System
- **Webhook Integration** - Extensible hook system for mail processing
  - `pre_receive` - Before accepting incoming mail
  - `post_receive` - After mail is stored
  - `pre_send` - Before sending outbound mail
  - `pre_delivery` - Before local/forward delivery
- **Plugin Architecture** - Support for third-party plugins (HTTP/gRPC)
- **AI-Ready** - Designed for AI workflow integration (spam detection, classification, summarization)

### Security & Spam Protection
- **rspamd Integration** - Full rspamd support for spam filtering
- **Rule-based Fallback** - 17+ built-in spam rules when rspamd is unavailable
- **HMAC Authentication** - Secure webhook/plugin communication
- **Circuit Breaker** - Resilient plugin execution

### Operations
- **Prometheus Metrics** - Built-in monitoring support
- **Structured Logging** - JSON-formatted logs for observability
- **Health Checks** - Comprehensive health endpoints
- **OpenAPI Documentation** - Interactive Swagger UI at `/docs`

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        MaiRust                              │
├─────────────────────────────────────────────────────────────┤
│  mairust-api     │  REST API Server (Axum)                  │
│  mairust-core    │  SMTP Server, Queue, Email Auth, Spam    │
│  mairust-storage │  Database & File Storage Abstraction     │
│  mairust-common  │  Shared Types & Configuration            │
└─────────────────────────────────────────────────────────────┘
         │                    │                    │
    ┌────▼────┐          ┌────▼────┐         ┌────▼────┐
    │PostgreSQL│         │   S3    │         │ Plugins │
    │ /SQLite  │         │Local FS │         │  rspamd │
    └──────────┘         └─────────┘         └─────────┘
```

## Quick Start

### Prerequisites

- Rust 1.75 or later
- PostgreSQL 14+

### Installation

```bash
# Clone the repository
git clone https://github.com/kani-momonga/MaiRust.git
cd MaiRust

# Build
cargo build --release

# Run tests
cargo test

# Start the server
cargo run --bin mairust
```

### Configuration

Create a configuration file at `/etc/mairust/config.yaml` or `./config.yaml`:

```yaml
server:
  smtp_port: 25
  submission_port: 587
  api_port: 8080

database:
  type: postgres  # or sqlite
  url: "postgres://user:password@localhost/mairust"

storage:
  type: fs  # or s3
  path: /var/lib/mairust/mail

tls:
  cert_path: /etc/mairust/cert.pem
  key_path: /etc/mairust/key.pem
```

### Docker

```bash
# Using docker-compose (coming soon)
docker-compose up -d
```

## API Usage

### Health Check

```bash
curl http://localhost:8080/health
```

### Create a Tenant

```bash
curl -X POST http://localhost:8080/admin/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "example", "display_name": "Example Corp"}'
```

### Send an Email

```bash
curl -X POST http://localhost:8080/tenants/{tenant_id}/send \
  -H "Content-Type: application/json" \
  -d '{
    "from": "sender@example.com",
    "to": ["recipient@example.com"],
    "subject": "Hello from MaiRust",
    "body": "This is a test email."
  }'
```

Full API documentation is available at `/docs` (Swagger UI) or `/openapi.json`.

## Roadmap

### Phase 1 - MVP (Current)
- [x] SMTP inbound/outbound with STARTTLS
- [x] SMTP AUTH (PLAIN/LOGIN)
- [x] SPF/DKIM/DMARC support
- [x] REST API for management
- [x] PostgreSQL storage
- [x] rspamd integration
- [x] OpenAPI documentation

### Phase 2 - Core Platform Expansion
- [ ] Multi-domain support enhancements
- [ ] Full-text search (Meilisearch integration)
- [ ] Admin Dashboard
- [ ] Advanced policy system
- [ ] IMAP support (read-only)

### Phase 3 - Full Mailbox Experience
- [ ] Full IMAP/POP support
- [ ] AI categorization plugins
- [ ] Message threading and tagging
- [ ] Web client UI
- [ ] Plugin system beta

### Phase 4 - Enterprise & Cloud
- [ ] Horizontal scaling / cluster mode
- [ ] Kubernetes Operator
- [ ] S3 recommended architecture
- [ ] SaaS control plane

### Phase 5 - Ecosystem Expansion
- [ ] Plugin marketplace
- [ ] JMAP support
- [ ] External integrations (Slack, Teams, etc.)

## Plugin Development

MaiRust supports external plugins via HTTP/gRPC. Plugins receive webhook payloads and can:
- Tag/classify messages
- Reject or quarantine spam
- Trigger external notifications
- Perform AI-powered analysis

Example plugin manifest (`plugin.toml`):

```toml
id = "com.example.mairust.my-plugin"
name = "My Plugin"
version = "1.0.0"

[entry]
type = "hook"
protocol = "http"
endpoint = "http://localhost:8081/hook"

[hooks]
on = ["post_receive"]

[permissions]
read_headers = true
read_body = "preview"
write_tags = true
```

## System Requirements

### Minimum (Development/Small)
- 2 vCPU
- 2 GB RAM
- 20 GB disk

### Recommended (Production)
- 4+ vCPU
- 8+ GB RAM
- Separate storage for DB and mail objects

### Supported Platforms
- **Linux (x86_64, aarch64)** - Officially supported
- **macOS** - Development/testing (best effort)
- **Windows** - Development only (not recommended for production)

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development

```bash
# Check compilation
cargo check

# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy
```

## License

MaiRust is licensed under the [Apache License 2.0](LICENSE).

Plugins can use any license (MIT, Apache-2.0, GPL, Proprietary, etc.).

## Comparison with Alternatives

| Feature | MaiRust | Postfix | Dovecot | Stalwart |
|---------|---------|---------|---------|----------|
| Language | Rust | C | C | Rust |
| API-first | Yes | No | No | Partial |
| Plugin System | Yes (HTTP/gRPC) | Limited | Yes | Limited |
| AI Integration | Designed for | No | No | No |
| Modern Config | YAML | Complex | Complex | TOML |
| License | Apache-2.0 | IBM/EPL | MIT/LGPL | AGPL |

## Links

- [Documentation](docs/)
- [Architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Plugin Architecture](docs/plugin-architecture.md)

## Acknowledgments

Built with excellent Rust crates including:
- [Tokio](https://tokio.rs/) - Async runtime
- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [SQLx](https://github.com/launchbadge/sqlx) - Database toolkit
- [mail-parser](https://github.com/stalwartlabs/mail-parser) - MIME parsing
