# MaiRust Deployment Guide (Non-Docker)

This document explains how to deploy MaiRust without Docker for production-like environments. It covers prerequisites, build steps, configuration, and a reference systemd unit so you can run the mail stack as a service.

## 1. Prerequisites
- **Rust:** Version 1.75 or later (matches `rust-version` in `Cargo.toml`).
- **Database:** PostgreSQL 14+ (recommended) or SQLite (single-node/testing).
- **Optional storage:** S3-compatible object storage; otherwise local filesystem path such as `/var/lib/mairust/mail`.
- **TLS assets:** Certificate and private key if you plan to enable STARTTLS/HTTPS.
- **Host environment:**
  - Open SMTP ports 25 and 587, API port 8080, and any IMAP/POP3 ports you enable.
  - DNS records (MX/SPF/DKIM/DMARC) configured for your domain.

## 2. Build the binaries
Clone and compile the workspace in release mode:

```bash
# Clone the repository
cd /opt
sudo git clone https://github.com/kani-momonga/MaiRust.git
cd MaiRust

# Build the MaiRust binary
cargo build --release -p mairust-server

# The compiled binary will be at
ls target/release/mairust
```

## 3. Create runtime user and directories
Run MaiRust as an unprivileged user and prepare data directories:

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin mairust || true
sudo mkdir -p /etc/mairust /var/lib/mairust/mail
sudo chown -R mairust:mairust /etc/mairust /var/lib/mairust
```

## 4. Configure MaiRust
1. Copy the provided example configuration and adjust values for your environment:
   ```bash
   sudo cp config.example.toml /etc/mairust/config.toml
   sudo chown mairust:mairust /etc/mairust/config.toml
   ```
2. Key settings (see inline comments in `config.example.toml`):
   - **[database]:** Set `backend = "postgres"` with a PostgreSQL URL, or `backend = "sqlite"` with a file path.
   - **[storage]:** Choose `backend = "fs"` with `path = "/var/lib/mairust/mail"`, or configure S3.
   - **[smtp]:** Host bindings and ports (25/587) plus TLS/auth requirements.
   - **[api]:** REST API port (default 8080) and CORS origins for the web UI.
   - **[imap]/[pop3]:** Enable and bind IMAP/POP3 if required.
   - **[tls]:** Provide `cert_path` and `key_path` to enable STARTTLS/HTTPS.
3. Configuration discovery: at runtime MaiRust searches for `./config.yaml`, `./config.toml`, `/etc/mairust/config.yaml`, then `/etc/mairust/config.toml`. Place your file in one of these paths.

## 5. Database preparation
- **PostgreSQL:** Create the database and user, then ensure the connection URL in your config matches. Migrations run automatically on startup.
  ```bash
  sudo -u postgres psql -c "CREATE DATABASE mairust;"
  sudo -u postgres psql -c "CREATE USER mairust WITH PASSWORD 'change_me';"
  sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE mairust TO mairust;"
  ```
- **SQLite:** Ensure the configured file path is writable by the `mairust` user; the file will be created on first start.

## 6. Run with systemd (recommended)
Create a service unit at `/etc/systemd/system/mairust.service`:

```ini
[Unit]
Description=MaiRust Mail Server
After=network.target postgresql.service

[Service]
User=mairust
Group=mairust
ExecStart=/opt/MaiRust/target/release/mairust
WorkingDirectory=/opt/MaiRust
Environment="RUST_LOG=info"
Restart=on-failure
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

Apply and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mairust
sudo systemctl status mairust
```

## 7. Operational checklist
- SMTP listening on ports 25 and 587; IMAP/POP3 if enabled.
- API reachable on port 8080 (Swagger UI at `/docs`).
- Log output in structured JSON (configurable via `[logging]`).
- Ensure firewalls and reverse proxies allow the chosen ports.
- Rotate TLS keys and keep `config.toml` permissions restricted to the `mairust` user.

## 8. Maintenance
- Rebuild after dependency or code updates: `git pull` then `cargo build --release -p mairust-server`.
- Restart service after deployments: `sudo systemctl restart mairust`.
- Database migrations run at startup; monitor logs for migration output.
