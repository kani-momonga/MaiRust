# 013: スケジュールメール送信機能 (Scheduled Email Sending)

**ステータス**: Draft
**最終更新**: 2024-12-11
**担当領域**: Backend / API / Queue

---

## 概要

MaiRust にスケジュールメール送信機能を追加します。メールマガジン配信、一斉送信、レート制限付き分散配信などの機能を提供し、大量のメールを効率的かつ安全に配信できるようにします。

### 主な機能

1. **キャンペーン管理**: メールマガジンなどの一斉配信を管理
2. **配信リスト**: 受信者リストの登録・管理
3. **スケジュール配信**: 指定日時での送信予約
4. **レート制限**: 時間あたりの送信数制限（例: 5000通/時）
5. **分散配信**: 大量送信を時間的に分散

---

## 1. アーキテクチャ概要

```
┌────────────────────────────────────────────────────────────────┐
│                        API Layer                               │
├────────────────────────────────────────────────────────────────┤
│  POST /campaigns          - キャンペーン作成                    │
│  POST /campaigns/:id/send - キャンペーン送信開始                │
│  POST /recipient-lists    - 配信リスト作成                      │
│  POST /scheduled-emails   - スケジュール送信作成                 │
└───────────────────────────┬────────────────────────────────────┘
                            │
┌───────────────────────────▼────────────────────────────────────┐
│                    Campaign Manager                            │
├────────────────────────────────────────────────────────────────┤
│  - キャンペーンの状態管理                                        │
│  - 配信リストの展開                                             │
│  - レート制限の計算                                             │
│  - スケジュールジョブの生成                                      │
└───────────────────────────┬────────────────────────────────────┘
                            │
┌───────────────────────────▼────────────────────────────────────┐
│                  Scheduled Job Queue                           │
├────────────────────────────────────────────────────────────────┤
│  scheduled_messages テーブル                                    │
│  - scheduled_at による送信時刻管理                               │
│  - batch_id によるグループ化                                    │
│  - status による状態追跡                                        │
└───────────────────────────┬────────────────────────────────────┘
                            │
┌───────────────────────────▼────────────────────────────────────┐
│                   Delivery Scheduler                           │
├────────────────────────────────────────────────────────────────┤
│  - 定期的にキューをポーリング（5秒間隔）                          │
│  - レート制限チェック                                           │
│  - 送信ジョブの実行                                             │
│  - 結果の記録                                                   │
└────────────────────────────────────────────────────────────────┘
```

---

## 2. データベーススキーマ

### 2.1 キャンペーンテーブル

```sql
-- キャンペーン（メールマガジンなどの一斉送信単位）
CREATE TABLE campaigns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- 基本情報
    name VARCHAR(255) NOT NULL,
    description TEXT,

    -- メール内容
    subject VARCHAR(998) NOT NULL,
    from_address VARCHAR(254) NOT NULL,
    from_name VARCHAR(255),
    reply_to VARCHAR(254),
    html_body TEXT,
    text_body TEXT,

    -- 送信設定
    recipient_list_id UUID REFERENCES recipient_lists(id),

    -- スケジュール設定
    scheduled_at TIMESTAMPTZ,           -- NULL = 即時送信

    -- レート制限設定
    rate_limit_per_hour INTEGER DEFAULT 5000,   -- 1時間あたりの送信上限
    rate_limit_per_minute INTEGER DEFAULT 100,  -- 1分あたりの送信上限

    -- 状態
    status VARCHAR(50) NOT NULL DEFAULT 'draft',
    -- draft: 下書き
    -- scheduled: 予約済み
    -- sending: 送信中
    -- paused: 一時停止
    -- completed: 完了
    -- cancelled: キャンセル
    -- failed: 失敗

    -- 統計
    total_recipients INTEGER DEFAULT 0,
    sent_count INTEGER DEFAULT 0,
    delivered_count INTEGER DEFAULT 0,
    bounced_count INTEGER DEFAULT 0,
    failed_count INTEGER DEFAULT 0,
    opened_count INTEGER DEFAULT 0,
    clicked_count INTEGER DEFAULT 0,

    -- メタデータ
    tags JSONB DEFAULT '[]',
    metadata JSONB DEFAULT '{}',

    -- タイムスタンプ
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- 制約
    CONSTRAINT valid_status CHECK (status IN ('draft', 'scheduled', 'sending', 'paused', 'completed', 'cancelled', 'failed'))
);

CREATE INDEX idx_campaigns_tenant_id ON campaigns(tenant_id);
CREATE INDEX idx_campaigns_status ON campaigns(status);
CREATE INDEX idx_campaigns_scheduled_at ON campaigns(scheduled_at) WHERE status = 'scheduled';
```

### 2.2 配信リストテーブル

```sql
-- 配信リスト
CREATE TABLE recipient_lists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    name VARCHAR(255) NOT NULL,
    description TEXT,

    -- 統計
    recipient_count INTEGER DEFAULT 0,
    active_count INTEGER DEFAULT 0,

    -- メタデータ
    metadata JSONB DEFAULT '{}',

    -- タイムスタンプ
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_recipient_lists_tenant_id ON recipient_lists(tenant_id);

-- 配信リストの受信者
CREATE TABLE recipients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    recipient_list_id UUID NOT NULL REFERENCES recipient_lists(id) ON DELETE CASCADE,

    email VARCHAR(254) NOT NULL,
    name VARCHAR(255),

    -- 状態
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    -- active: 有効
    -- unsubscribed: 配信停止
    -- bounced: バウンス
    -- complained: 苦情

    -- パーソナライズ用データ
    attributes JSONB DEFAULT '{}',

    -- タイムスタンプ
    subscribed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    unsubscribed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 一意性制約
    CONSTRAINT unique_recipient_per_list UNIQUE (recipient_list_id, email),
    CONSTRAINT valid_recipient_status CHECK (status IN ('active', 'unsubscribed', 'bounced', 'complained'))
);

CREATE INDEX idx_recipients_list_id ON recipients(recipient_list_id);
CREATE INDEX idx_recipients_email ON recipients(email);
CREATE INDEX idx_recipients_status ON recipients(status);
```

### 2.3 スケジュールメッセージテーブル

```sql
-- スケジュールされた個別メッセージ
CREATE TABLE scheduled_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- 関連
    campaign_id UUID REFERENCES campaigns(id) ON DELETE CASCADE,
    recipient_id UUID REFERENCES recipients(id),
    batch_id UUID,  -- 同じバッチでグループ化

    -- メール内容
    from_address VARCHAR(254) NOT NULL,
    to_address VARCHAR(254) NOT NULL,
    subject VARCHAR(998) NOT NULL,
    html_body TEXT,
    text_body TEXT,
    headers JSONB DEFAULT '{}',

    -- スケジュール
    scheduled_at TIMESTAMPTZ NOT NULL,

    -- 状態
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    -- pending: 待機中
    -- processing: 処理中
    -- sent: 送信済み
    -- delivered: 配信確認
    -- bounced: バウンス
    -- failed: 失敗
    -- cancelled: キャンセル

    -- 実行情報
    attempts INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 3,
    last_attempt_at TIMESTAMPTZ,
    last_error TEXT,

    -- 配信結果
    message_id VARCHAR(255),  -- 送信後のMessage-ID
    sent_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    bounced_at TIMESTAMPTZ,
    bounce_type VARCHAR(50),
    bounce_reason TEXT,

    -- メタデータ
    metadata JSONB DEFAULT '{}',

    -- タイムスタンプ
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_scheduled_status CHECK (status IN ('pending', 'processing', 'sent', 'delivered', 'bounced', 'failed', 'cancelled'))
);

CREATE INDEX idx_scheduled_messages_tenant_id ON scheduled_messages(tenant_id);
CREATE INDEX idx_scheduled_messages_campaign_id ON scheduled_messages(campaign_id);
CREATE INDEX idx_scheduled_messages_batch_id ON scheduled_messages(batch_id);
CREATE INDEX idx_scheduled_messages_status ON scheduled_messages(status);
CREATE INDEX idx_scheduled_messages_scheduled_at ON scheduled_messages(scheduled_at)
    WHERE status = 'pending';
CREATE INDEX idx_scheduled_messages_to_address ON scheduled_messages(to_address);
```

### 2.4 レート制限テーブル

```sql
-- テナントごとのレート制限状態
CREATE TABLE rate_limit_counters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- 時間枠
    window_type VARCHAR(20) NOT NULL,  -- 'minute', 'hour', 'day'
    window_start TIMESTAMPTZ NOT NULL,

    -- カウント
    count INTEGER NOT NULL DEFAULT 0,

    -- 制限値
    limit_value INTEGER NOT NULL,

    -- タイムスタンプ
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT unique_rate_limit_window UNIQUE (tenant_id, window_type, window_start)
);

CREATE INDEX idx_rate_limit_counters_tenant_window ON rate_limit_counters(tenant_id, window_type, window_start);

-- テナントのレート制限設定
CREATE TABLE tenant_rate_limits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- 制限値
    per_minute INTEGER DEFAULT 100,
    per_hour INTEGER DEFAULT 5000,
    per_day INTEGER DEFAULT 50000,

    -- 有効化
    enabled BOOLEAN DEFAULT true,

    -- タイムスタンプ
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT unique_tenant_rate_limit UNIQUE (tenant_id)
);
```

### 2.5 配信停止（Unsubscribe）テーブル

```sql
-- グローバル配信停止リスト
CREATE TABLE unsubscribes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    email VARCHAR(254) NOT NULL,

    -- 配信停止元
    source VARCHAR(50) NOT NULL,  -- 'manual', 'link', 'bounce', 'complaint'
    campaign_id UUID REFERENCES campaigns(id),

    -- 理由
    reason TEXT,

    -- タイムスタンプ
    unsubscribed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT unique_unsubscribe UNIQUE (tenant_id, email)
);

CREATE INDEX idx_unsubscribes_tenant_email ON unsubscribes(tenant_id, email);
```

---

## 3. API エンドポイント

### 3.1 キャンペーン API

```
# キャンペーン管理
POST   /api/v1/tenants/{tenant_id}/campaigns                    # キャンペーン作成
GET    /api/v1/tenants/{tenant_id}/campaigns                    # キャンペーン一覧
GET    /api/v1/tenants/{tenant_id}/campaigns/{id}               # キャンペーン詳細
PUT    /api/v1/tenants/{tenant_id}/campaigns/{id}               # キャンペーン更新
DELETE /api/v1/tenants/{tenant_id}/campaigns/{id}               # キャンペーン削除

# キャンペーン操作
POST   /api/v1/tenants/{tenant_id}/campaigns/{id}/send          # 送信開始
POST   /api/v1/tenants/{tenant_id}/campaigns/{id}/schedule      # スケジュール設定
POST   /api/v1/tenants/{tenant_id}/campaigns/{id}/pause         # 一時停止
POST   /api/v1/tenants/{tenant_id}/campaigns/{id}/resume        # 再開
POST   /api/v1/tenants/{tenant_id}/campaigns/{id}/cancel        # キャンセル
GET    /api/v1/tenants/{tenant_id}/campaigns/{id}/stats         # 統計情報

# プレビュー・テスト
POST   /api/v1/tenants/{tenant_id}/campaigns/{id}/preview       # プレビュー生成
POST   /api/v1/tenants/{tenant_id}/campaigns/{id}/test-send     # テスト送信
```

### 3.2 配信リスト API

```
# 配信リスト管理
POST   /api/v1/tenants/{tenant_id}/recipient-lists              # リスト作成
GET    /api/v1/tenants/{tenant_id}/recipient-lists              # リスト一覧
GET    /api/v1/tenants/{tenant_id}/recipient-lists/{id}         # リスト詳細
PUT    /api/v1/tenants/{tenant_id}/recipient-lists/{id}         # リスト更新
DELETE /api/v1/tenants/{tenant_id}/recipient-lists/{id}         # リスト削除

# 受信者管理
POST   /api/v1/tenants/{tenant_id}/recipient-lists/{id}/recipients          # 受信者追加
GET    /api/v1/tenants/{tenant_id}/recipient-lists/{id}/recipients          # 受信者一覧
PUT    /api/v1/tenants/{tenant_id}/recipient-lists/{id}/recipients/{rid}    # 受信者更新
DELETE /api/v1/tenants/{tenant_id}/recipient-lists/{id}/recipients/{rid}    # 受信者削除

# 一括操作
POST   /api/v1/tenants/{tenant_id}/recipient-lists/{id}/import  # CSV/JSON インポート
GET    /api/v1/tenants/{tenant_id}/recipient-lists/{id}/export  # エクスポート
```

### 3.3 スケジュール送信 API

```
# 単発スケジュール送信（キャンペーンなしの予約送信）
POST   /api/v1/tenants/{tenant_id}/scheduled-emails             # スケジュール作成
GET    /api/v1/tenants/{tenant_id}/scheduled-emails             # スケジュール一覧
GET    /api/v1/tenants/{tenant_id}/scheduled-emails/{id}        # スケジュール詳細
PUT    /api/v1/tenants/{tenant_id}/scheduled-emails/{id}        # スケジュール更新
DELETE /api/v1/tenants/{tenant_id}/scheduled-emails/{id}        # スケジュールキャンセル
```

### 3.4 配信停止 API

```
# 配信停止管理
POST   /api/v1/tenants/{tenant_id}/unsubscribes                 # 配信停止追加
GET    /api/v1/tenants/{tenant_id}/unsubscribes                 # 配信停止一覧
DELETE /api/v1/tenants/{tenant_id}/unsubscribes/{email}         # 配信停止解除

# 公開エンドポイント（認証不要）
GET    /unsubscribe/{token}                                     # 配信停止ページ
POST   /unsubscribe/{token}                                     # 配信停止実行
```

---

## 4. リクエスト・レスポンス形式

### 4.1 キャンペーン作成

**リクエスト:**
```json
POST /api/v1/tenants/{tenant_id}/campaigns
{
  "name": "2024年1月ニュースレター",
  "description": "月次のお知らせメール",
  "subject": "【{{company_name}}】1月のお知らせ",
  "from_address": "newsletter@example.com",
  "from_name": "Example Newsletter",
  "reply_to": "support@example.com",
  "html_body": "<html><body><h1>こんにちは、{{name}}さん</h1>...</body></html>",
  "text_body": "こんにちは、{{name}}さん\n...",
  "recipient_list_id": "550e8400-e29b-41d4-a716-446655440000",
  "scheduled_at": "2024-01-15T09:00:00+09:00",
  "rate_limit_per_hour": 5000,
  "tags": ["newsletter", "january"],
  "metadata": {
    "template_version": "2.0"
  }
}
```

**レスポンス:**
```json
{
  "data": {
    "id": "660e8400-e29b-41d4-a716-446655440001",
    "name": "2024年1月ニュースレター",
    "status": "draft",
    "total_recipients": 10000,
    "scheduled_at": "2024-01-15T09:00:00+09:00",
    "rate_limit_per_hour": 5000,
    "estimated_completion": "2024-01-15T11:00:00+09:00",
    "created_at": "2024-01-10T10:30:00Z"
  }
}
```

### 4.2 スケジュール送信（単発）

**リクエスト:**
```json
POST /api/v1/tenants/{tenant_id}/scheduled-emails
{
  "from": "sender@example.com",
  "to": ["recipient@example.com"],
  "subject": "スケジュールされたメール",
  "html_body": "<p>このメールは予約送信されました。</p>",
  "text_body": "このメールは予約送信されました。",
  "scheduled_at": "2024-01-20T15:00:00+09:00"
}
```

**レスポンス:**
```json
{
  "data": {
    "id": "770e8400-e29b-41d4-a716-446655440002",
    "status": "pending",
    "scheduled_at": "2024-01-20T15:00:00+09:00",
    "created_at": "2024-01-10T10:30:00Z"
  }
}
```

### 4.3 キャンペーン統計

**リクエスト:**
```
GET /api/v1/tenants/{tenant_id}/campaigns/{id}/stats
```

**レスポンス:**
```json
{
  "data": {
    "campaign_id": "660e8400-e29b-41d4-a716-446655440001",
    "status": "sending",
    "total_recipients": 10000,
    "sent": 4500,
    "delivered": 4200,
    "bounced": 50,
    "failed": 30,
    "opened": 1200,
    "clicked": 350,
    "unsubscribed": 15,
    "progress_percentage": 45.0,
    "estimated_completion": "2024-01-15T11:00:00+09:00",
    "current_rate": 4800,
    "rate_limit_per_hour": 5000
  }
}
```

---

## 5. 分散配信アルゴリズム

### 5.1 配信スケジューリング

大量のメールを配信する際、レート制限に基づいて送信時刻を分散させます。

```rust
/// 配信スケジュールを計算
fn calculate_schedule(
    total_recipients: usize,
    rate_per_hour: usize,
    rate_per_minute: usize,
    start_time: DateTime<Utc>,
) -> Vec<ScheduledBatch> {
    let mut batches = Vec::new();
    let mut remaining = total_recipients;
    let mut current_time = start_time;

    // 1分あたりのバッチサイズ
    let batch_size = rate_per_minute;

    while remaining > 0 {
        let count = remaining.min(batch_size);
        batches.push(ScheduledBatch {
            scheduled_at: current_time,
            count,
        });
        remaining -= count;
        current_time += Duration::minutes(1);

        // 1時間の制限チェック
        // 60バッチごとに時間制限を確認
    }

    batches
}
```

### 5.2 送信時刻の分散

```
例: 10,000通、5,000通/時、100通/分 の場合

開始時刻: 09:00:00
├── Batch 1:  09:00:00 - 100通
├── Batch 2:  09:01:00 - 100通
├── Batch 3:  09:02:00 - 100通
│   ...
├── Batch 50: 09:49:00 - 100通 (5,000通完了、1時間目終了)
│
│   [1時間経過を待つ]
│
├── Batch 51: 10:00:00 - 100通
├── Batch 52: 10:01:00 - 100通
│   ...
└── Batch 100: 10:49:00 - 100通 (10,000通完了)

完了時刻: 10:49:00
所要時間: 約1時間50分
```

### 5.3 動的レート調整

配信中にバウンスや失敗が多発した場合、自動的にレートを下げる機能：

```rust
struct AdaptiveRateLimiter {
    base_rate: usize,
    current_rate: usize,
    error_threshold: f64,  // エラー率の閾値（例: 0.05 = 5%）
    min_rate: usize,       // 最低レート
}

impl AdaptiveRateLimiter {
    fn adjust_rate(&mut self, recent_errors: usize, recent_total: usize) {
        let error_rate = recent_errors as f64 / recent_total as f64;

        if error_rate > self.error_threshold {
            // エラー率が高い場合、レートを50%に下げる
            self.current_rate = (self.current_rate / 2).max(self.min_rate);
        } else if error_rate < self.error_threshold / 2.0 {
            // エラー率が低い場合、徐々にレートを戻す
            self.current_rate = (self.current_rate + 10).min(self.base_rate);
        }
    }
}
```

---

## 6. バックグラウンド処理

### 6.1 Scheduler Worker

```rust
/// スケジュール配信ワーカー
async fn scheduled_delivery_worker(
    db: PgPool,
    smtp_client: SmtpClient,
    rate_limiter: RateLimiter,
) {
    let interval = Duration::from_secs(5);

    loop {
        // 送信予定時刻を過ぎた pending メッセージを取得
        let messages = sqlx::query_as!(
            ScheduledMessage,
            r#"
            SELECT * FROM scheduled_messages
            WHERE status = 'pending'
              AND scheduled_at <= NOW()
            ORDER BY scheduled_at ASC
            LIMIT 100
            FOR UPDATE SKIP LOCKED
            "#
        )
        .fetch_all(&db)
        .await?;

        for msg in messages {
            // レート制限チェック
            if !rate_limiter.check(msg.tenant_id).await? {
                continue;  // 次のサイクルで再試行
            }

            // 送信処理
            let result = send_message(&smtp_client, &msg).await;

            // 結果を記録
            update_message_status(&db, &msg, result).await?;

            // レート制限カウンターを更新
            rate_limiter.increment(msg.tenant_id).await?;
        }

        tokio::time::sleep(interval).await;
    }
}
```

### 6.2 Campaign Status Updater

```rust
/// キャンペーン状態更新ワーカー
async fn campaign_status_worker(db: PgPool) {
    let interval = Duration::from_secs(30);

    loop {
        // 送信中のキャンペーンを取得
        let campaigns = sqlx::query_as!(
            Campaign,
            r#"
            SELECT * FROM campaigns
            WHERE status = 'sending'
            "#
        )
        .fetch_all(&db)
        .await?;

        for campaign in campaigns {
            // 統計を更新
            let stats = calculate_campaign_stats(&db, campaign.id).await?;

            // キャンペーンの状態を更新
            if stats.sent_count >= stats.total_recipients {
                update_campaign_status(&db, campaign.id, "completed").await?;
            }
        }

        tokio::time::sleep(interval).await;
    }
}
```

---

## 7. テンプレート変数

### 7.1 サポートする変数

キャンペーンメールでは、受信者ごとにパーソナライズが可能です：

| 変数 | 説明 | 例 |
|------|------|-----|
| `{{email}}` | メールアドレス | user@example.com |
| `{{name}}` | 受信者名 | 山田太郎 |
| `{{first_name}}` | 名 | 太郎 |
| `{{last_name}}` | 姓 | 山田 |
| `{{unsubscribe_url}}` | 配信停止URL | https://... |
| `{{attributes.xxx}}` | カスタム属性 | 任意の値 |

### 7.2 テンプレート処理

```rust
fn render_template(template: &str, recipient: &Recipient) -> String {
    let mut result = template.to_string();

    result = result.replace("{{email}}", &recipient.email);
    result = result.replace("{{name}}", &recipient.name.unwrap_or_default());

    // カスタム属性の置換
    if let Some(attributes) = &recipient.attributes {
        for (key, value) in attributes.as_object().unwrap_or(&serde_json::Map::new()) {
            let placeholder = format!("{{{{attributes.{}}}}}", key);
            result = result.replace(&placeholder, &value.to_string());
        }
    }

    // 配信停止URLの生成
    let unsubscribe_token = generate_unsubscribe_token(recipient);
    let unsubscribe_url = format!("https://mail.example.com/unsubscribe/{}", unsubscribe_token);
    result = result.replace("{{unsubscribe_url}}", &unsubscribe_url);

    result
}
```

---

## 8. 配信停止（Unsubscribe）処理

### 8.1 配信停止URL

各メールに配信停止リンクを含める：

```html
<a href="{{unsubscribe_url}}">配信停止はこちら</a>
```

### 8.2 List-Unsubscribe ヘッダー

RFC 8058 に準拠した One-Click Unsubscribe をサポート：

```
List-Unsubscribe: <mailto:unsubscribe@example.com?subject=unsubscribe>, <https://mail.example.com/unsubscribe/TOKEN>
List-Unsubscribe-Post: List-Unsubscribe=One-Click
```

### 8.3 配信時のチェック

送信前に配信停止リストを確認：

```rust
async fn should_send(db: &PgPool, tenant_id: Uuid, email: &str) -> bool {
    let unsubscribed = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM unsubscribes WHERE tenant_id = $1 AND email = $2)",
        tenant_id,
        email
    )
    .fetch_one(db)
    .await
    .unwrap_or(false);

    !unsubscribed.unwrap_or(false)
}
```

---

## 9. エラーハンドリング

### 9.1 リトライ戦略

| 失敗タイプ | リトライ | 最大試行回数 | 間隔 |
|-----------|---------|-------------|------|
| 一時的エラー (4xx) | あり | 3回 | 指数バックオフ |
| 接続エラー | あり | 3回 | 指数バックオフ |
| 永続的エラー (5xx bounce) | なし | - | - |
| レート制限 | あり | 無制限 | 動的調整 |

### 9.2 バウンス処理

```rust
enum BounceType {
    Hard,   // 永続的（アドレス不存在など）
    Soft,   // 一時的（メールボックス満杯など）
}

async fn handle_bounce(db: &PgPool, message: &ScheduledMessage, bounce_type: BounceType) {
    match bounce_type {
        BounceType::Hard => {
            // 受信者を bounced に更新
            update_recipient_status(db, message.recipient_id, "bounced").await;
            // 配信停止リストに追加
            add_to_unsubscribe(db, message.tenant_id, &message.to_address, "bounce").await;
        }
        BounceType::Soft => {
            // リトライをスケジュール
            reschedule_message(db, message.id, Duration::hours(1)).await;
        }
    }
}
```

---

## 10. 監視とアラート

### 10.1 メトリクス

| メトリクス | 説明 |
|-----------|------|
| `scheduled_emails_pending` | 送信待ちメール数 |
| `scheduled_emails_sent_total` | 送信完了数 |
| `scheduled_emails_failed_total` | 失敗数 |
| `scheduled_emails_bounce_rate` | バウンス率 |
| `campaign_progress` | キャンペーン進捗 |
| `rate_limit_current` | 現在のレート |

### 10.2 アラート条件

- バウンス率が 5% を超えた場合
- 送信失敗率が 1% を超えた場合
- キャンペーンが予定より 30% 以上遅延している場合
- レート制限により送信が滞っている場合

---

## 11. セキュリティ考慮事項

### 11.1 認証・認可

- すべてのAPIエンドポイントで API Key 認証を要求
- テナント間のデータアクセスを厳密に分離
- 配信停止URLには署名付きトークンを使用

### 11.2 スパム防止

- 送信前に配信停止リストをチェック
- バウンスした宛先への再送信を防止
- 苦情（complaint）が発生した宛先をブロック

### 11.3 データ保護

- 受信者の個人情報は暗号化して保存
- 配信停止リクエストは即座に処理
- GDPR/個人情報保護法への準拠

---

## 12. 設定項目

```toml
[scheduled_email]
# ワーカー設定
worker_interval_secs = 5
batch_size = 100

# デフォルトレート制限
default_rate_per_minute = 100
default_rate_per_hour = 5000
default_rate_per_day = 50000

# リトライ設定
max_retry_attempts = 3
retry_backoff_base_secs = 60

# テンプレート
max_template_size_bytes = 1048576  # 1MB
max_recipients_per_campaign = 100000

# 配信停止
unsubscribe_token_expiry_days = 365
```

---

## 13. 今後の拡張

### Phase 1 (MVP)
- 基本的なキャンペーン作成・送信
- 配信リスト管理
- スケジュール送信
- レート制限

### Phase 2
- テンプレート変数のサポート
- 配信停止機能
- 基本的な統計

### Phase 3
- A/Bテスト
- 開封・クリックトラッキング
- 高度な分析ダッシュボード
- Webhook通知（送信完了、バウンスなど）

---

## 関連ドキュメント

- [003-api.md](./003-api.md) - API設計
- [005-hooks.md](./005-hooks.md) - Hook Manager
- [011-multitenancy.md](./011-multitenancy.md) - マルチテナンシー
