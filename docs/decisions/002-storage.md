# 002: ストレージ設計 (Storage)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: Backend

---

## 概要

MaiRust のストレージアーキテクチャを定義します。データベース、オブジェクトストレージ、検索エンジンの設計と、データライフサイクル管理について記載します。

---

## 1. ストレージアーキテクチャ概要

```
┌─────────────────────────────────────────────────────────────┐
│                      MaiRust Core                           │
├─────────────────────────────────────────────────────────────┤
│  StorageBackend Trait                                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │   fs        │  │    s3       │  │  (future plugins)   │ │
│  │  (local)    │  │  (S3互換)   │  │                     │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
         │                  │
         ▼                  ▼
┌─────────────────┐  ┌─────────────────┐
│  Local FS       │  │  S3 / MinIO     │
│  /var/lib/...   │  │  Wasabi, etc.   │
└─────────────────┘  └─────────────────┘
```

---

## 2. データベース

### 2.1 PostgreSQL（推奨）

**用途:**
- メタデータ（users, domains, mailboxes, hooks, plugins）
- メールインデックス（headers, envelope, flags）
- ジョブキュー
- 監査ログ

**推奨バージョン:** PostgreSQL 14+

**接続設定例:**
```yaml
database:
  backend: "postgres"
  url: "postgres://user:password@localhost:5432/mairust"
  max_connections: 20
  min_connections: 5
```

### 2.2 SQLite（開発・小規模用）

**用途:**
- 開発・検証環境
- 単一ノード・小規模運用
- 組み込み構成

**制限事項:**
- 同時書き込みに弱い
- レプリケーション非対応
- 大規模データには不向き

**設定例:**
```yaml
database:
  backend: "sqlite"
  path: "/var/lib/mairust/mairust.db"
```

### 2.3 DB 抽象化

`StorageBackend` トレイトで抽象化し、以下を公式サポート：
- `postgres`
- `sqlite`
- 将来: `mysql`（コミュニティ貢献として）

---

## 3. オブジェクトストレージ

### 3.1 設計方針

- メール本体は「不変オブジェクト」として保存
- 更新は新オブジェクト作成（上書きしない）
- メタデータ（タグ、フラグ等）は DB で管理

### 3.2 ローカルファイルシステム (`fs`)

**デフォルト設定:**
```yaml
storage:
  backend: "fs"
  options:
    base_path: "/var/lib/mairust/mail"
```

**ディレクトリ構造:**
```
/var/lib/mairust/mail/
├── <tenant_id>/
│   ├── <mailbox_id>/
│   │   ├── <message_id>.eml
│   │   └── <message_id>/
│   │       └── attachments/
│   │           └── <attachment_id>
```

### 3.3 S3 互換ストレージ (`s3`)

**要件:**
- S3 API v4 署名対応
- 必要な操作: `PUT`, `GET`, `HEAD`, `DELETE`, マルチパートアップロード

**設定例:**
```yaml
storage:
  backend: "s3"
  options:
    bucket: "mairust-mails"
    region: "ap-northeast-1"
    endpoint: "https://s3.ap-northeast-1.amazonaws.com"
    access_key_id: "env:MAIRUST_S3_ACCESS_KEY"
    secret_access_key: "env:MAIRUST_S3_SECRET_KEY"
```

**テスト対象:**
- AWS S3（リファレンス実装）
- MinIO（CI テスト用）
- Wasabi（可能であれば）

### 3.4 マルチパートアップロード

| 設定 | デフォルト | 説明 |
|------|-----------|------|
| `multipart_threshold_mb` | 8 | この値以上でマルチパート |
| `multipart_chunk_mb` | 8 | 分割サイズ |

**設定例:**
```yaml
storage:
  backend: "s3"
  options:
    multipart_threshold_mb: 8
    multipart_chunk_mb: 8
```

### 3.5 暗号化

#### サーバーサイド暗号化（SSE）

| 方式 | 用途 |
|------|------|
| `sse-s3` | デフォルト。S3 側の鍵管理 |
| `sse-kms` | 企業向け。KMS で鍵管理 |

**設定例:**
```yaml
storage:
  backend: "s3"
  options:
    encryption: "sse-s3"
    # または
    # encryption: "sse-kms"
    # kms_key_id: "arn:aws:kms:..."
```

#### クライアントサイド暗号化

- 上級者オプションとして提供
- 検索・インデックスとの相性が悪い
- 非常にセンシティブな環境向け

---

## 4. 検索エンジン

### 4.1 位置づけ

- **オプション機能**（なしでも動作）
- フルテキスト検索の高速化
- 複雑なフィルタ・AI連携の絞り込み

### 4.2 Meilisearch（推奨）

**長所:**
- セットアップが簡単
- シンプルな API
- 単一コンテナで完結

**短所:**
- 超大規模（数億通）には限界あり

**設定例:**
```yaml
search:
  backend: "meilisearch"
  url: "http://localhost:7700"
  api_key: "env:MAIRUST_MEILISEARCH_KEY"
```

### 4.3 Elasticsearch / OpenSearch

**長所:**
- 大規模・高スケール対応
- 複雑なクエリに強い
- 既存運用があれば活用可能

**短所:**
- 運用が重い（クラスタ構築、JVM チューニング）

**設定例:**
```yaml
search:
  backend: "elasticsearch"
  url: "https://localhost:9200"
  username: "elastic"
  password: "env:MAIRUST_ES_PASSWORD"
```

### 4.4 構成パターン

| 構成 | 検索エンジン | 用途 |
|------|-------------|------|
| 最小構成 | なし | 開発、超小規模 |
| 標準構成 | Meilisearch | 小〜中規模 |
| 大規模構成 | Elasticsearch | 大規模、エンタープライズ |

---

## 5. データライフサイクル

### 5.1 メッセージの状態遷移

```
[受信] → [保存] → [論理削除] → [物理削除]
                      │
                      └→ [アーカイブ] (オプション)
```

### 5.2 論理削除

- DB に `deleted_at` タイムスタンプを設定
- UI/API からは非表示
- バックアップ・監査のために保持

### 5.3 物理削除

- 定期クリーンアップジョブで実行
- `deleted_at` から一定期間経過後

**設定:**
```yaml
retention:
  deleted_messages_days: 30  # デフォルト
```

### 5.4 S3 ライフサイクルとの連携

S3 側のライフサイクルルールと併用可能：

```json
{
  "Rules": [
    {
      "ID": "ArchiveOldMessages",
      "Status": "Enabled",
      "Filter": { "Prefix": "archive/" },
      "Transitions": [
        { "Days": 90, "StorageClass": "GLACIER" }
      ]
    }
  ]
}
```

---

## 6. バックアップ

### 6.1 推奨手順

1. **DB バックアップ**
   ```bash
   pg_dump -Fc mairust > mairust_$(date +%Y%m%d).dump
   ```

2. **メール本体バックアップ**
   - ローカルFS: `rsync` やスナップショット
   - S3: バージョニング有効化、クロスリージョンレプリケーション

3. **設定バックアップ**
   - `/etc/mairust/` 以下をバージョン管理

### 6.2 将来のコマンド

```bash
mairust backup --output=/backup/mairust_20240115.tar
mairust restore --input=/backup/mairust_20240115.tar
```

---

## 7. 障害時の挙動

### 7.1 DB 接続不可時

- 新規メール受信: `tempfail` (451) を返す
- API リクエスト: `503 Service Unavailable`
- 管理者にアラート通知

### 7.2 S3 接続不可時

- 新規メール受信: `tempfail` (451) を返す
- メール閲覧: エラー表示（「本文を取得できません」）
- メタデータ（ヘッダ等）は DB から表示可能

### 7.3 検索エンジン接続不可時

- 検索機能: 利用不可（エラー表示）
- 他の機能: 正常動作
- フォールバックとして DB の LIKE 検索を提供（オプション）

---

## 8. パフォーマンス指標

### 8.1 目標値（単一ノード）

| 指標 | 目標 |
|------|------|
| メッセージ保存 | 100〜500 msg/sec |
| メッセージ取得 | 1000+ msg/sec |
| 検索（Meilisearch） | < 100ms (p95) |

### 8.2 スケーリング

- **垂直スケーリング**: CPU/メモリ増強
- **水平スケーリング**: 複数ノード + 共有ストレージ（S3）

---

## 関連ドキュメント

- [001-foundation.md](./001-foundation.md) - 基本方針
- [009-container.md](./009-container.md) - コンテナ配布
- [010-operations.md](./010-operations.md) - 運用設計
