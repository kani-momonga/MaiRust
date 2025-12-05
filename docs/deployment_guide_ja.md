# MaiRust デプロイガイド（非Docker）

本書はDockerを使わずにMaiRustをデプロイする手順をまとめています。必要な前提条件、ビルド方法、設定、systemdによる常駐化までをカバーします。

## 1. 前提条件
- **Rust:** バージョン1.75以上（`Cargo.toml`の指定と一致）
- **データベース:** PostgreSQL 14+（推奨）またはSQLite（単一ノード/テスト用途）
- **任意のストレージ:** S3互換オブジェクトストレージ。使用しない場合は`/var/lib/mairust/mail`などのローカルパス。
- **TLS証明書:** STARTTLS/HTTPSを有効にする場合に必要。
- **ホスト環境:**
  - SMTP(25/587)やAPI(8080)、有効化したIMAP/POP3ポートが開放されていること。
  - ドメインのDNSレコード（MX/SPF/DKIM/DMARC）が正しく設定されていること。

## 2. バイナリのビルド
リリースビルドでコンパイルします。

```bash
# リポジトリ取得
cd /opt
sudo git clone https://github.com/kani-momonga/MaiRust.git
cd MaiRust

# MaiRustバイナリをビルド
cargo build --release -p mairust-server

# ビルド成果物
ls target/release/mairust
```

## 3. 実行ユーザーとディレクトリの準備
専用ユーザーで実行し、データディレクトリを作成します。

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin mairust || true
sudo mkdir -p /etc/mairust /var/lib/mairust/mail
sudo chown -R mairust:mairust /etc/mairust /var/lib/mairust
```

## 4. 設定ファイル
1. サンプル設定を配置して環境に合わせて編集します。
   ```bash
   sudo cp config.example.toml /etc/mairust/config.toml
   sudo chown mairust:mairust /etc/mairust/config.toml
   ```
2. 主な設定ポイント（`config.example.toml`のコメントも参照）:
   - **[database]:** `backend = "postgres"` と接続URL、または `backend = "sqlite"` とファイルパス。
   - **[storage]:** `backend = "fs"` とローカルパス、またはS3設定。
   - **[smtp]:** バインド先とポート(25/587)、TLS/認証要件。
   - **[api]:** REST APIポート(デフォルト8080)とWeb UI向けのCORS。
   - **[imap]/[pop3]:** 必要に応じて有効化とバインド設定。
   - **[tls]:** STARTTLS/HTTPSを使う場合の証明書パス。
3. 設定ファイルの探索順序: `./config.yaml`、`./config.toml`、`/etc/mairust/config.yaml`、`/etc/mairust/config.toml`。これらのいずれかに配置してください。

## 5. データベース準備
- **PostgreSQL:** DBとユーザーを作成し、設定ファイルの接続URLを一致させます。マイグレーションは起動時に自動実行されます。
  ```bash
  sudo -u postgres psql -c "CREATE DATABASE mairust;"
  sudo -u postgres psql -c "CREATE USER mairust WITH PASSWORD 'change_me';"
  sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE mairust TO mairust;"
  ```
- **SQLite:** 設定したファイルパスに`mairust`ユーザーが書き込み可能であることを確認してください（初回起動時にファイルが作成されます）。

## 6. systemdでの起動（推奨）
`/etc/systemd/system/mairust.service`を作成します。

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

サービスを適用・起動します。

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mairust
sudo systemctl status mairust
```

## 7. 運用チェックリスト
- SMTPは25/587番ポートで待ち受け、必要ならIMAP/POP3も有効化。
- APIは8080番ポートで利用可能（Swagger UIは`/docs`）。
- ログは構造化JSONで出力（`[logging]`で調整可能）。
- ファイアウォール/リバースプロキシで必要なポートを許可。
- TLS鍵のローテーションと`config.toml`の権限管理を徹底。

## 8. メンテナンス
- 依存更新やコード変更後は `git pull`、`cargo build --release -p mairust-server` で再ビルド。
- デプロイ後は `sudo systemctl restart mairust` で再起動。
- マイグレーションは起動時に実行されるため、ログで結果を確認してください。
