# MaiRust Development Roadmap

This document outlines the development plan and phases for the MaiRust project.

---

## Phase 1 — MVP (Foundations)
**Goal:** Deliver minimal but functional mail platform.

### Features
- SMTP inbound (basic queueing)
- Local delivery storage (PostgreSQL + FS)
- REST API (auth, users, domains, send mail)
- Simple Web Admin UI
- AI Worker (one feature: summarization or spam detection)

### Milestones
- `mairust init` CLI prototype
- Basic SMTP transaction compliance

---

## Phase 2 — Core Platform Expansion
**Goal:** Support practical real-world use.

### Additions
- Multi-domain support
- DKIM/SPF signing/validation
- Search capability (Meilisearch integration)
- Admin Dashboard (metrics, logs)
- Policy system (rules, routing)

### Milestones
- Structured storage abstraction
- API stability guarantee

---

## Phase 3 — Full Mailbox Experience
**Goal:** Become a real alternative to traditional mail stacks.

### Additions
- IMAP/POP backend (Rust-native or adapter)
- Advanced AI categorization
- Message threading, tagging
- Web client UI enhancements (MUA-like)
- Plugin system beta

### Milestones
- First community plugin
- Full test suite for mailbox protocols

---

## Phase 4 — Enterprise & Cloud
**Goal:** Scalable, distributed, production-grade platform.

### Additions
- Cluster mode / horizontal scaling
- Object storage (S3) recommended architecture
- Kubernetes Operator
- SaaS control plane support

### Milestones
- Multi-node cluster passing reliability tests
- Public SaaS preview

---

## Phase 5 — Ecosystem Expansion
**Goal:** Make MaiRust the modern mail ecosystem.

### Additions
- Marketplace for plugins (AI models, filters, UI themes)
- JMAP support for modern clients
- External integrations (Slack, Teams, Webhook rules)

### Milestones
- 50+ community plugins
- Official integrations catalog

