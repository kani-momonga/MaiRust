# 005: Hook Manager 設計 (Hooks)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: Backend (Core)

---

## 概要

MaiRust の Hook Manager 設計を定義します。Hook の種類、実行フロー、タイムアウト制御、Circuit Breaker、結果処理について記載します。

---

## 1. Hook の概要

### 1.1 Hook とは

メール処理の特定のタイミングで外部ロジック（プラグイン）を呼び出すための仕組み。

### 1.2 Hook の種類

| Hook | タイミング | 処理モード | 用途 |
|------|-----------|-----------|------|
| `pre_receive` | SMTP DATA 受信後、保存前 | **同期** | スパム拒否、ポリシーチェック |
| `post_receive` | メール保存後 | **非同期** | 分類、通知、AI処理 |
| `pre_send` | 送信キュー投入前 | **同期** | DLP、承認フロー |
| `pre_delivery` | ローカル配送直前 | **同期** | フォルダ振り分け、タグ付け |

### 1.3 同期 vs 非同期

| モード | 特徴 |
|--------|------|
| **同期** | SMTP セッション内で完結。タイムアウトが厳しい |
| **非同期** | キュー経由で実行。時間のかかる処理向け |

---

## 2. Hook Manager アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                      MaiRust Core                           │
├─────────────────────────────────────────────────────────────┤
│                      Hook Manager                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │  Dispatcher │  │  Executor   │  │  Circuit Breaker    │ │
│  │  (順序制御) │  │  (実行)     │  │  (障害検知)         │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
│                          │                                   │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                  Result Aggregator                      ││
│  │                  (結果集約・判定)                        ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
           │                    │                    │
           ▼                    ▼                    ▼
    ┌──────────┐         ┌──────────┐         ┌──────────┐
    │ Plugin A │         │ Plugin B │         │ Plugin C │
    │ (HTTP)   │         │ (gRPC)   │         │ (STDIN)  │
    └──────────┘         └──────────┘         └──────────┘
```

---

## 3. Hook 設定

### 3.1 設定例（YAML）

```yaml
hooks:
  - id: "spam-filter"
    type: "pre_receive"
    plugin_id: "com.example.spam-filter"
    enabled: true
    priority: 10
    timeout_ms: 2000
    on_timeout: "allow"
    filter:
      recipient_matches: ".*@example.com"

  - id: "ai-classifier"
    type: "post_receive"
    plugin_id: "com.example.ai-classifier"
    enabled: true
    priority: 20
```

### 3.2 設定項目

| 項目 | 説明 | デフォルト |
|------|------|-----------|
| `id` | Hook の一意識別子 | (必須) |
| `type` | Hook タイプ | (必須) |
| `plugin_id` | 呼び出すプラグイン | (必須) |
| `enabled` | 有効/無効 | `true` |
| `priority` | 実行優先度（小さいほど先） | `100` |
| `timeout_ms` | タイムアウト | Hook タイプによる |
| `on_timeout` | タイムアウト時の動作 | `tempfail` |
| `on_error` | エラー時の動作 | `allow` |
| `filter` | 実行条件 | (なし) |

---

## 4. 実行順序

### 4.1 優先度ルール

1. `priority` 昇順（小さいほど先）
2. 同一 priority 内では `id` の辞書順

### 4.2 実行例

```
priority=10: spam-filter
priority=20: virus-scan
priority=100: ai-classifier
```

---

## 5. タイムアウト制御

### 5.1 Hook タイプ別デフォルト

| Hook | デフォルト | 最大 |
|------|-----------|------|
| `pre_receive` | 2秒 | **3秒** |
| `post_receive` | 30秒 | 300秒 |
| `pre_send` | 2秒 | 5秒 |
| `pre_delivery` | 2秒 | 5秒 |

### 5.2 タイムアウト時の動作

| 設定値 | 動作 |
|--------|------|
| `allow` | 処理を続行（デフォルトで許可） |
| `reject` | 拒否（5xx） |
| `tempfail` | 一時拒否（4xx） |

**推奨:**
- `pre_receive`: AI 系は `allow`、セキュリティ系は `tempfail`
- `pre_send`: DLP 系は `reject` または `tempfail`

---

## 6. Circuit Breaker

### 6.1 目的

プラグインの連続失敗時に呼び出しを自動停止し、システム全体の安定性を保護。

### 6.2 状態遷移

```
[Closed] ──失敗閾値超過──▶ [Open]
    ▲                        │
    │                        │ 一定時間経過
    │                        ▼
    └───成功──── [Half-Open]
```

### 6.3 閾値設定

#### 通常の Hook

| 条件 | 閾値 |
|------|------|
| 連続失敗 | 10回 |
| 失敗率（5分窓） | 80% |

#### pre_receive 専用（厳格モード）

| 条件 | 閾値 |
|------|------|
| 連続タイムアウト/エラー | **3回** |
| 失敗率（1分窓） | 70% |

### 6.4 Circuit Open 時の動作

```yaml
circuit_breaker:
  on_open: "skip"  # skip | reject | tempfail
```

| 設定 | 動作 |
|------|------|
| `skip` | プラグインをスキップして処理続行（デフォルト） |
| `reject` | 拒否 |
| `tempfail` | 一時拒否 |

### 6.5 回復

- **自動 Half-Open**: Open から 60秒後に1リクエストだけ試行
- **手動リセット**: Admin UI/CLI から強制的に Closed へ

---

## 7. 結果の処理

### 7.1 プラグインからの応答形式

```json
{
  "action": "allow",
  "tags": ["spam:low", "category:newsletter"],
  "score": 0.3,
  "metadata": {
    "classifier_version": "1.2.0"
  }
}
```

### 7.2 アクション種類

| アクション | 説明 |
|-----------|------|
| `allow` | 許可（デフォルト） |
| `reject` | 永続的拒否（5xx） |
| `tempfail` | 一時拒否（4xx） |
| `tag` | タグ付け（許可） |
| `quarantine` | 隔離フォルダへ移動（許可） |

### 7.3 複数プラグインの結果合成

#### 優先度ルール

```
reject > tempfail > quarantine > tag > allow
```

#### 合成ロジック

1. `reject`/`tempfail` を返したプラグインがあれば **即座に中断**
2. `tag` は全プラグインの結果を **マージ**（重複除去）
3. `score` は **最大値** を採用（または平均、設定可能）
4. `metadata` は **マージ**（プラグイン ID でネームスペース化）

### 7.4 SMTP レスポンスコード

| アクション | デフォルトコード | 説明 |
|-----------|-----------------|------|
| `reject` | `550 5.7.1` | Message rejected |
| `tempfail` | `451 4.7.1` | Try again later |

**カスタムコード:**
```json
{
  "action": "reject",
  "smtp_code": 554,
  "smtp_enhanced": "5.7.1",
  "smtp_message": "Spam detected"
}
```

---

## 8. Hook ペイロード

### 8.1 MaiRust → プラグイン（リクエスト）

```json
{
  "hook_type": "post_receive",
  "message_id": "msg_abc123",
  "tenant_id": "tenant_001",
  "mailbox": "user@example.com",
  "envelope": {
    "from": "sender@example.com",
    "to": ["user@example.com"]
  },
  "headers": {
    "Subject": "Hello",
    "Date": "Mon, 15 Jan 2024 10:30:00 +0900",
    "Message-Id": "<abc123@example.com>",
    "From": "sender@example.com",
    "To": "user@example.com"
  },
  "body": {
    "preview": "Hello, this is a sample message...",
    "size": 12345,
    "has_attachments": true,
    "attachments": [
      {
        "filename": "document.pdf",
        "content_type": "application/pdf",
        "size": 102400
      }
    ]
  },
  "callback_url": "https://mairust.local/internal/plugins/callback/msg_abc123",
  "metadata": {
    "received_at": "2024-01-15T10:30:00Z",
    "client_ip": "192.168.1.100"
  }
}
```

### 8.2 プラグイン → MaiRust（レスポンス / コールバック）

```json
{
  "plugin_id": "com.example.spam-filter",
  "action": "tag",
  "tags": ["spam:low"],
  "score": 0.3,
  "metadata": {
    "rules_matched": ["rule_001", "rule_002"]
  }
}
```

---

## 9. フィルタ条件

### 9.1 設定例

```yaml
hooks:
  - id: "support-handler"
    type: "post_receive"
    plugin_id: "com.example.ticket-creator"
    filter:
      recipient_matches: "support@.*"
      sender_not_matches: ".*@internal.example.com"
      has_attachments: true
```

### 9.2 フィルタ項目

| 項目 | 説明 |
|------|------|
| `recipient_matches` | 受信者アドレス（正規表現） |
| `recipient_not_matches` | 受信者アドレス否定 |
| `sender_matches` | 送信者アドレス |
| `sender_not_matches` | 送信者アドレス否定 |
| `subject_matches` | 件名 |
| `has_attachments` | 添付ファイルの有無 |
| `size_gt` | メッセージサイズ（超） |
| `size_lt` | メッセージサイズ（未満） |

---

## 10. 非同期 Hook の再試行

### 10.1 post_receive の再試行

| 項目 | 設定 |
|------|------|
| 最大再試行回数 | 3回 |
| バックオフ | 30秒 → 2分 → 10分 |
| 失敗時 | ログ記録、permanent-failed としてマーク |

### 10.2 設定

```yaml
hooks:
  - id: "ai-classifier"
    type: "post_receive"
    plugin_id: "com.example.ai"
    retry:
      max_attempts: 3
      backoff_seconds: [30, 120, 600]
```

---

## 11. メトリクス

### 11.1 提供メトリクス

| メトリクス | 説明 |
|-----------|------|
| `mairust_hook_calls_total` | Hook 呼び出し総数 |
| `mairust_hook_duration_seconds` | Hook 実行時間 |
| `mairust_hook_errors_total` | エラー数 |
| `mairust_hook_timeouts_total` | タイムアウト数 |
| `mairust_circuit_breaker_state` | Circuit Breaker 状態 |

### 11.2 ラベル

- `hook_type`: pre_receive, post_receive, etc.
- `plugin_id`: プラグイン ID
- `action`: allow, reject, tempfail, etc.

---

## 12. Admin UI / API

### 12.1 機能

- Hook 一覧・詳細表示
- 有効/無効の切り替え
- テスト実行（サンプルメッセージで Hook を呼び出し）
- メトリクス・ログの表示
- Circuit Breaker 状態の確認・リセット

### 12.2 API エンドポイント

```
GET    /api/v1/admin/hooks
POST   /api/v1/admin/hooks
GET    /api/v1/admin/hooks/:id
PUT    /api/v1/admin/hooks/:id
DELETE /api/v1/admin/hooks/:id
POST   /api/v1/admin/hooks/:id/test
POST   /api/v1/admin/hooks/:id/circuit-breaker/reset
```

---

## 関連ドキュメント

- [006-plugins.md](./006-plugins.md) - プラグインアーキテクチャ
- [008-outbound-spam.md](./008-outbound-spam.md) - 送信スパム対策
- [003-api.md](./003-api.md) - API 設計
