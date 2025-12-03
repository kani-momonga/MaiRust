# MaiRust Architecture

## Overview
MaiRust is a modern, API-driven, AI-enabled mail platform built in Rust.  
This document describes the core architectural components, the hook & plugin system, and how they interact.

---

## Core Components

### 1. MaiRust Core (Rust)
- SMTP server (inbound/outbound)
- Queue manager
- Routing engine
- Storage abstraction layer (DB + object storage)
- Security modules (SPF, DKIM, DMARC)
- **Hook Manager**
  - Executes configured hooks at `pre_receive`, `post_receive`, `pre_delivery`, `pre_send`
  - Dispatches events to plugins (local scripts, HTTP/gRPC services, webhooks)
- **Plugin Runtime Integration**
  - Validates plugin metadata (`plugin.toml`)
  - Enforces permission scopes (headers-only / body-preview / full-body, etc.)
  - Provides internal callback endpoints for plugin responses

### 2. API Server
- REST / GraphQL
- User & domain management
- Mailbox operations (list/search/read/tag/move)
- Policy & routing configuration
- **Hook & Plugin Management APIs**
  - CRUD for hooks (`/admin/hooks`)
  - Plugin lifecycle (`/admin/plugins`: install, enable/disable, upgrade)
  - Exposure of internal callback endpoints for plugin results

### 3. Web Admin UI
- Next.js / React
- Authentication (admin / tenant / user)
- Dashboard, search, tagging
- **Hook & Plugin Management UI**
  - Hook list/view/edit, enable/disable, test execution
  - Plugin gallery (local & marketplace-backed)
  - Plugin details (permissions, changelog, logs, metrics)

### 4. Plugins / Workers
> External processes/services implementing MaiRust’s plugin interface.

- Implemented in Python, Rust, Go, etc.
- Types:
  - Hook Plugins（同期 or 準同期処理）
  - Service Plugins（非同期AI・バッチ処理）
- Typical responsibilities:
  - Classification, summarization, spam detection, translation
  - Ticket/issue creation, chat notifications, DLP, custom workflows
- Integration:
  - Receive hook payloads via HTTP/gRPC/STDIN
  - Return immediate actions or call back later via the callback API
- Async job processing via queue (NATS/Redis/RabbitMQ etc.)

### 5. Storage Layer
- PostgreSQL or SQLite for metadata（users, domains, mailboxes, hooks, plugins, logs）
- S3/local FS for message bodies & attachments
- Optional full-text search (Meilisearch/Elasticsearch) for message content
- Optional object metadata store for plugin results（tags, scores, AI annotations）

### 6. (Future) Marketplace Integration
- External Marketplace API client
  - Discover / search plugins compatible with current MaiRust version
  - Download & verify plugin packages
- Local plugin catalog & cache
- License/entitlement verification (for paid plugins, future)

---

## Data Flow

### 1. Inbound Mail
1. SMTP inbound → MaiRust Core (SMTP server)
2. **`pre_receive` hooks** (optional, synchronous)
   - Core → Hook Manager → Configured plugins
   - Plugins can allow/reject/tempfail or annotate the message
3. Message accepted → Stored in Storage Layer
4. **`post_receive` hooks** (usually async)
   - Hook Manager enqueues jobs or calls plugins directly
   - Plugins:
     - Return immediate actions (e.g., tag/folder)
     - Or perform long-running processing and call the **callback API** when done
5. Plugin results → Core updates metadata (tags, scores, flags, indexes)

### 2. User/API Access
1. User / system → API Server (REST/GraphQL)
2. API Server → Storage Layer (messages, metadata)
3. Optional plugin-triggered actions:
   - On-demand AI actions (summarize / classify / translate a message)
   - Triggered via API → plugin → callback

### 3. Outbound Mail
1. User/API → `send` endpoint or SMTP submission
2. **`pre_send` hooks**:
   - Policy checks, compliance/DLP, auto-signature insertion, BCC injection etc.
3. Message queued → SMTP outbound
4. **`pre_delivery` hooks** (for local/forward delivery)
   - Folder selection, tagging, extra headers

---

## Protocols and Standards
- SMTP / Submission / LMTP
- IMAP/POP (future)
- JMAP (future)
- DKIM / SPF / DMARC
- Webhook signing (HMAC-based, for plugin & external integrations)
- OAuth2/OIDC / API tokens for plugin & admin authentication (future)

---

## Non-Goals (Initially)
- Full Exchange-like groupware (calendar, tasks, contacts sync)
- Built-in groupware UI（will rely on integrations/plugins instead）
- Direct monolithic AI機能（AIは基本的にプラグインとして扱う）

---

## Future Extensions
- **Richer Plugin Architecture**
  - UI plugins (custom panels, buttons, views)
  - WASM-based sandboxed plugins
  - Multi-tenant plugin configuration
- Multi-region deployment
- SaaS control plane
- Private & public plugin marketplaces
- JMAP-based clients with deep plugin integration
