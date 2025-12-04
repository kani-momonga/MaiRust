# 009: コンテナ配布・デプロイ (Container & Deployment)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: DevOps / Infra

---

## 概要

MaiRust のコンテナ配布とデプロイ戦略を定義します。オールインワン構成、本番構成、Kubernetes デプロイについて記載します。

---

## 1. 配布形式

### 1.1 提供イメージ

| イメージ | 用途 |
|---------|------|
| `mairust/all-in-one` | 開発・小規模・評価用 |
| `mairust/core` | 本番用（Core のみ） |
| `mairust/api` | 本番用（API Server） |
| `mairust/web` | 本番用（Web UI） |

### 1.2 タグ戦略

| タグ | 説明 |
|-----|------|
| `latest` | 最新の安定版 |
| `x.y.z` | 特定バージョン |
| `x.y` | マイナーバージョン最新 |
| `edge` | 開発版（不安定） |

### 1.3 マルチアーキテクチャ

- `linux/amd64`
- `linux/arm64`

---

## 2. オールインワン構成

### 2.1 目的

```
「docker run mairust で、外部依存なしの最小構成がすぐ動く」
```

### 2.2 含まれるコンポーネント

```
mairust/all-in-one:latest
├── mairust-core    (SMTP サーバー)
├── mairust-api     (REST API)
├── mairust-web     (Web UI)
├── PostgreSQL      (組み込み)
└── Supervisord     (プロセス管理)
```

### 2.3 クイックスタート

```bash
docker run -d \
  --name mairust \
  -p 25:25 \
  -p 587:587 \
  -p 8080:8080 \
  -v mairust_data:/var/lib/mairust \
  mairust/all-in-one:latest
```

### 2.4 docker-compose

```yaml
version: '3.8'

services:
  mairust:
    image: mairust/all-in-one:latest
    ports:
      - "25:25"      # SMTP
      - "587:587"    # Submission
      - "8080:8080"  # Web UI / API
    volumes:
      - mairust_db:/var/lib/mairust/db
      - mairust_mail:/var/lib/mairust/mail
      - ./config.yaml:/etc/mairust/config.yaml:ro
    environment:
      - MAIRUST_ADMIN_EMAIL=admin@example.com
      - MAIRUST_ADMIN_PASSWORD=changeme
    restart: unless-stopped

volumes:
  mairust_db:
  mairust_mail:
```

### 2.5 永続化パス

| パス | 内容 |
|------|------|
| `/var/lib/mairust/db` | PostgreSQL データ |
| `/var/lib/mairust/mail` | メール本体 |
| `/var/log/mairust` | ログ |
| `/etc/mairust` | 設定ファイル |

---

## 3. 本番構成

### 3.1 推奨構成

```
┌─────────────────────────────────────────────────────────────┐
│                        Load Balancer                         │
│                    (nginx / HAProxy / ALB)                   │
└─────────────────────────────────────────────────────────────┘
           │                    │                    │
           ▼                    ▼                    ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ mairust-core     │  │ mairust-api      │  │ mairust-web      │
│ (SMTP)           │  │ (REST API)       │  │ (Web UI)         │
└──────────────────┘  └──────────────────┘  └──────────────────┘
           │                    │                    │
           └────────────────────┼────────────────────┘
                               ▼
                    ┌──────────────────┐
                    │ PostgreSQL       │
                    │ (マネージド推奨)  │
                    └──────────────────┘
                               │
                    ┌──────────────────┐
                    │ S3 / MinIO       │
                    │ (オブジェクト)    │
                    └──────────────────┘
```

### 3.2 docker-compose (本番)

```yaml
version: '3.8'

services:
  core:
    image: mairust/core:latest
    ports:
      - "25:25"
      - "587:587"
    environment:
      - DATABASE_URL=postgres://mairust:password@db:5432/mairust
      - STORAGE_BACKEND=s3
      - S3_ENDPOINT=http://minio:9000
    depends_on:
      - db
      - minio

  api:
    image: mairust/api:latest
    ports:
      - "8081:8080"
    environment:
      - DATABASE_URL=postgres://mairust:password@db:5432/mairust
    depends_on:
      - db

  web:
    image: mairust/web:latest
    ports:
      - "8080:80"
    environment:
      - API_URL=http://api:8080

  db:
    image: postgres:15
    volumes:
      - postgres_data:/var/lib/postgresql/data
    environment:
      - POSTGRES_USER=mairust
      - POSTGRES_PASSWORD=password
      - POSTGRES_DB=mairust

  minio:
    image: minio/minio
    command: server /data --console-address ":9001"
    volumes:
      - minio_data:/data
    environment:
      - MINIO_ROOT_USER=minioadmin
      - MINIO_ROOT_PASSWORD=minioadmin

volumes:
  postgres_data:
  minio_data:
```

---

## 4. Kubernetes デプロイ

### 4.1 Helm Chart

```bash
# リポジトリ追加
helm repo add mairust https://charts.mairust.io
helm repo update

# インストール
helm install mairust mairust/mairust \
  --namespace mairust \
  --create-namespace \
  --values values.yaml
```

### 4.2 values.yaml 例

```yaml
global:
  storageClass: "standard"

core:
  replicas: 2
  resources:
    requests:
      cpu: "500m"
      memory: "512Mi"
    limits:
      cpu: "2000m"
      memory: "2Gi"

api:
  replicas: 3
  resources:
    requests:
      cpu: "250m"
      memory: "256Mi"

web:
  replicas: 2

postgresql:
  enabled: false  # 外部 DB を使用
  external:
    host: "postgres.example.com"
    database: "mairust"
    username: "mairust"
    existingSecret: "mairust-db-credentials"

storage:
  backend: "s3"
  s3:
    bucket: "mairust-mails"
    region: "ap-northeast-1"
    existingSecret: "mairust-s3-credentials"

ingress:
  enabled: true
  className: "nginx"
  hosts:
    - host: mail.example.com
      paths:
        - path: /
          pathType: Prefix
```

### 4.3 Kubernetes Operator（Phase 4）

Phase 4 以降で Kubernetes Operator を提供予定：
- バージョン管理・ローリングアップデート
- 設定同期
- バックアップ連携
- スケーリング

---

## 5. 設定

### 5.1 環境変数

| 変数 | 説明 |
|------|------|
| `DATABASE_URL` | DB 接続文字列 |
| `STORAGE_BACKEND` | `fs` / `s3` |
| `S3_ENDPOINT` | S3 エンドポイント |
| `S3_ACCESS_KEY` | S3 アクセスキー |
| `S3_SECRET_KEY` | S3 シークレットキー |
| `MAIRUST_MASTER_KEY` | 暗号化マスターキー |
| `LOG_LEVEL` | ログレベル |

### 5.2 設定ファイル

```yaml
# /etc/mairust/config.yaml
server:
  smtp_port: 25
  submission_port: 587
  api_port: 8080

database:
  url: "postgres://..."

storage:
  backend: "s3"
  options:
    bucket: "mairust-mails"
```

---

## 6. イメージサイズ

### 6.1 目標

| イメージ | 目標サイズ |
|---------|-----------|
| all-in-one | < 500 MB |
| core | < 100 MB |
| api | < 100 MB |
| web | < 50 MB |

### 6.2 最適化

- マルチステージビルド
- Alpine ベース or distroless
- 不要ファイルの除外

---

## 7. マイグレーション

### 7.1 組み込み DB → 外部 DB

```bash
# 1. データエクスポート
docker exec mairust mairust db export --output /backup/

# 2. 外部 DB にインポート
mairust db import --input /backup/ --target postgres://...

# 3. 設定変更
# DATABASE_URL を外部 DB に変更

# 4. 再起動
docker-compose up -d
```

### 7.2 ローカルストレージ → S3

```bash
# 1. データ同期
mairust storage migrate --from=fs --to=s3

# 2. 設定変更
# STORAGE_BACKEND=s3

# 3. 再起動
```

---

## 8. ヘルスチェック

### 8.1 エンドポイント

| パス | 用途 |
|------|------|
| `/health` | 基本ヘルスチェック |
| `/health/ready` | Readiness（依存サービス含む） |
| `/health/live` | Liveness |

### 8.2 Kubernetes 設定

```yaml
livenessProbe:
  httpGet:
    path: /health/live
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 10

readinessProbe:
  httpGet:
    path: /health/ready
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 5
```

---

## 9. ログ

### 9.1 出力先

- **開発**: STDOUT/STDERR
- **本番**: STDOUT → ログ収集（Fluentd/Vector/Loki）

### 9.2 構造化ログ

```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "level": "info",
  "component": "smtp",
  "message": "Connection accepted",
  "fields": {
    "remote_addr": "192.168.1.100"
  }
}
```

---

## 10. セキュリティ

### 10.1 コンテナセキュリティ

- 非 root ユーザーで実行
- Read-only ファイルシステム（可能な限り）
- Capability の最小化

### 10.2 Dockerfile ベストプラクティス

```dockerfile
FROM rust:1.75 as builder
# ... ビルド ...

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/mairust /usr/local/bin/
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/mairust"]
```

---

## 関連ドキュメント

- [001-foundation.md](./001-foundation.md) - 基本方針
- [002-storage.md](./002-storage.md) - ストレージ設計
- [010-operations.md](./010-operations.md) - 運用設計
