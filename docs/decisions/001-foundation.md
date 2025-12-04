# 001: 基本方針 (Foundation)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: 全体

---

## 概要

MaiRust プロジェクトの基本方針を定義します。ライセンス、技術選択、対応環境、設定形式などの根本的な決定事項を記載します。

---

## 1. ライセンス

### 1.1 MaiRust 本体

**Apache-2.0** を採用します。

**理由:**
- 商用利用しやすい
- 特許条項をカバー
- OSS コミュニティでの採用実績が豊富

### 1.2 プラグイン

プラグイン開発者は自由にライセンスを選択可能：
- MIT
- Apache-2.0
- GPL 系
- Proprietary（商用プラグイン）

---

## 2. 技術選択

### 2.1 プログラミング言語

| コンポーネント | 言語 |
|---------------|------|
| Core / API Server | Rust |
| Web Admin UI | TypeScript (React / Next.js) |
| CLI | Rust |
| プラグイン | 任意（Rust, Python, Go, etc.） |

### 2.2 デフォルトデータベース

**PostgreSQL** をデフォルトとします。

| 用途 | 推奨DB |
|------|--------|
| 本番環境 | PostgreSQL |
| 開発・検証 | PostgreSQL または SQLite |
| 組み込み・小規模 | SQLite |

**SQLite 適用基準:**
- 単一ノード構成
- 同時接続ユーザー < 20
- メール通数 < 10万通/日
- 高可用性・レプリケーション不要

### 2.3 ストレージバックエンド

| バックエンド | 用途 |
|-------------|------|
| `fs` (ローカルFS) | デフォルト、小〜中規模 |
| `s3` (S3互換) | 大規模、クラウド環境 |

### 2.4 検索エンジン

| エンジン | 推奨用途 |
|---------|----------|
| なし（DB LIKE） | 最小構成 |
| Meilisearch | 小〜中規模（推奨） |
| Elasticsearch/OpenSearch | 大規模・既存運用あり |

### 2.5 キュー

| Phase | 実装 |
|-------|------|
| Phase 1 | PostgreSQL ベースの組み込みキュー |
| Phase 2+ | NATS / Redis / RabbitMQ（オプション） |

---

## 3. 対応環境

### 3.1 対応 OS

| OS | サポートレベル |
|----|---------------|
| Linux (x86_64, aarch64) | **公式サポート** |
| macOS | 開発・検証用（ベストエフォート） |
| BSD | ベストエフォート |
| Windows | 開発用のみ（本番非推奨） |

### 3.2 最小要件

#### 開発・小規模環境

| リソース | 要件 |
|----------|------|
| CPU | 2 vCPU |
| メモリ | 2 GB RAM |
| ディスク | 20 GB |

#### 中規模環境

| リソース | 要件 |
|----------|------|
| CPU | 4+ vCPU |
| メモリ | 8+ GB RAM |
| ディスク | DB・オブジェクトストレージは別途 |

---

## 4. 設定

### 4.1 設定ファイル形式

**YAML** を採用します。

**理由:**
- 人間に読み書きしやすい
- コメントが書ける
- 広く普及している

**設定ファイルパス:**
```
/etc/mairust/config.yaml      # システム全体
~/.config/mairust/config.yaml # ユーザー設定（開発用）
./config.yaml                 # カレントディレクトリ（開発用）
```

### 4.2 設定のバリデーション

- JSON Schema または Rust 構造体による厳密なバリデーション
- 起動時に設定エラーを検出し、明確なエラーメッセージを出力

### 4.3 ホットリロード

以下の設定は再起動なしで反映可能：
- Hooks / Plugins の有効・無効
- Rate Limit 設定
- Logging レベル

以下は再起動が必要：
- ポート番号
- DB 接続先
- TLS 証明書パス

**リロード方法:**
```bash
mairust reload
# または
kill -SIGHUP <pid>
```

---

## 5. ログ

### 5.1 フォーマット

**JSON 構造化ログ** をデフォルトとします。

```json
{
  "timestamp": "2024-01-15T10:30:00.000Z",
  "level": "info",
  "component": "smtp",
  "message": "Connection accepted",
  "request_id": "req_abc123",
  "remote_addr": "192.168.1.100"
}
```

**開発用オプション:**
- `--log-format=text` でプレーンテキスト出力に切り替え可能

### 5.2 主要フィールド

| フィールド | 説明 |
|-----------|------|
| `timestamp` | ISO 8601 形式 |
| `level` | trace, debug, info, warn, error |
| `component` | smtp, api, hook, plugin, etc. |
| `message` | ログメッセージ |
| `request_id` | リクエスト追跡用ID |
| `plugin_id` | プラグイン関連の場合 |
| `tenant_id` | テナント関連の場合 |

---

## 6. 国際化 (i18n)

### 6.1 対応言語

| Phase | 対応言語 |
|-------|----------|
| Phase 1 | 英語のみ |
| Phase 2+ | 日本語、英語 |
| 将来 | コミュニティ翻訳を受け入れ |

### 6.2 実装方針

- 文言は i18n フレンドリーな仕組みで管理（JSON/YAML）
- ハードコードされた文字列は避ける

---

## 7. エラーコード体系

### 7.1 REST API エラー

```json
{
  "error": {
    "code": "INVALID_REQUEST",
    "message": "The request body is invalid",
    "details": {
      "field": "email",
      "reason": "Invalid email format"
    }
  }
}
```

| HTTP Status | 用途 |
|-------------|------|
| 400 | リクエスト不正 |
| 401 | 認証エラー |
| 403 | 権限エラー |
| 404 | リソース不存在 |
| 409 | 競合（重複等） |
| 422 | バリデーションエラー |
| 429 | レート制限超過 |
| 500 | サーバー内部エラー |

### 7.2 SMTP エラー

| コード | 用途 |
|--------|------|
| 4xx | 一時的エラー（tempfail） |
| 5xx | 永続的エラー（permanent） |

プラグインから具体的なコードを指定可能：
- `reject` → デフォルト `550 5.7.1`
- `tempfail` → デフォルト `451 4.7.1`

---

## 8. Non-Goals（対象外）

Phase 1〜3 では以下を対象外とします：

- フル Exchange 互換のグループウェア機能
- CalDAV / CardDAV（カレンダー、連絡先同期）
- 組み込み AI 機能（AI はプラグインとして提供）
- Windows ネイティブサポート

---

## 関連ドキュメント

- [002-storage.md](./002-storage.md) - ストレージ設計の詳細
- [010-operations.md](./010-operations.md) - 運用設計
- [012-phase1-mvp.md](./012-phase1-mvp.md) - Phase 1 スコープ
