# Competitor Analysis

This document summarizes major competitors in the mail-server space and their characteristics.

---

## Traditional Mail Servers

### Postfix
- Extremely stable and secure
- Flexible routing and queueing
- Complex configuration files
- Lacks modern API integration

### Dovecot
- Leading IMAP/POP server
- Efficient mailbox storage
- Rich plugin ecosystem
- Still config-file-driven & not API-first

### Exim
- Highly configurable MTA
- Default in Debian family
- Powerful but complex rule system

### Sendmail
- Legacy enterprise MTA
- Historically influential
- Very difficult configuration

### OpenSMTPD
- Simplicity-focused design
- Secure codebase
- Limited ecosystem size

---

## All-in-One Systems

### Zimbra
- Enterprise suite (mail, calendar, contacts)
- Strong admin UI and API
- Heavyweight and Java-based

### Cyrus IMAP
- Scalable IMAP system
- Well-tested in large institutions
- Administration complexity is high

---

## Modern / SaaS Approaches

### Fastmail
- API-first attitude
- JMAP implementation
- Proprietary SaaS (not self-hosted)

### Gmail / Google Workspace
- AI filtering, excellent UX
- Not self-hostable
- Proprietary

---

## Rust-Based Competitors

### Stalwart Mail Server
- The most complete Rust mail server
- JMAP / IMAP / SMTP support
- Web UI included
- Ambitious but still growing
- Architecture differs: modules tightly coupled

### lettre (library)
- SMTP client library
- Not a server

### mailin / mailin-embedded
- SMTP server library
- Lacks full MTA/MDA capabilities

### mail-parser
- MIME parsing in Rust
- Useful as a building block

---

## Opportunity for MaiRust
- No API-first mail platform exists in OSS
- No mail server deeply designed for AI workflows
- Rust ecosystem has no dominant mail server yet
- Simplicity + Modular architecture = differentiation

