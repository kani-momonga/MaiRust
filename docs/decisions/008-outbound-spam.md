# 008: 送信スパム対策 (Outbound Spam Protection)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: Backend (Core)

---

## 概要

MaiRust の送信スパム対策設計を定義します。アカウント乗っ取り対策、レート制限、Outbound Policy Engine、監視サーバー連携について記載します。

---

## 1. 設計目標

### 1.1 ゴール

- アカウント乗っ取り時の被害最小化
- 送信元ドメインの評判（reputation）保護
- 正当な大量送信（ニュースレター等）への対応
- 外部監視サービスとの連携

### 1.2 対策レイヤー

```
[送信リクエスト]
      │
      ▼
┌─────────────────────────────────────┐
│ 1. Rate Limiter (レート制限)        │
└─────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────┐
│ 2. Outbound Policy Engine           │
│    (行動・コンテンツ・履歴分析)      │
└─────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────┐
│ 3. pre_send Hook (プラグイン)        │
│    (DLP、AI スコアリング等)          │
└─────────────────────────────────────┘
      │
      ▼
[送信キュー → SMTP 送信]
```

---

## 2. Outbound Policy Engine

### 2.1 アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                  Outbound Policy Engine                     │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │  Rate       │  │  Behavior   │  │  Content            │ │
│  │  Analyzer   │  │  Analyzer   │  │  Analyzer           │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
│         │                │                  │               │
│         └────────────────┼──────────────────┘               │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    Score Aggregator                     ││
│  │                    (スコア統合・判定)                    ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

### 2.2 シグナル

#### 行動系（Behavioral）

| シグナル | 説明 |
|---------|------|
| 短時間大量送信 | 1時間で数百〜数千通 |
| 同一コンテンツ繰り返し | 同一/類似本文の連続送信 |
| 異常な宛先数 | To/Cc/Bcc が極端に多い |
| 異常な地域 | 普段と異なる国/地域からのアクセス |
| 異常なクライアント | 普段と異なる UA/クライアント |

#### コンテンツ系（Content-based）

| シグナル | 説明 |
|---------|------|
| 既知スパムパターン | ルールベースのパターンマッチ |
| 危険 URL | ブラックリスト/脅威インテリジェンス |
| 不自然な構造 | 短文 + 画像のみ等 |

#### 履歴系（Reputation）

| シグナル | 説明 |
|---------|------|
| 過去のスパム判定 | 過去に拒否/警告されたメール数 |
| 管理者レポート | 手動でスパム報告されたアカウント |

### 2.3 スコアリング

```
score = w1 * rate_factor + w2 * content_factor + w3 * history_factor
```

**デフォルト重み:**

| ファクター | 重み |
|-----------|------|
| rate_factor | 0.4 |
| content_factor | 0.4 |
| history_factor | 0.2 |

**判定閾値:**

| スコア | アクション |
|--------|-----------|
| < 0.5 | 許可 |
| 0.5 - 0.8 | 警告・軽い制限 |
| >= 0.8 | 送信停止候補 |

---

## 3. レート制限

### 3.1 デフォルト制限

| 対象 | 制限 |
|------|------|
| ユーザー/時間 | 200 通/時 |
| ユーザー/日 | 1000 通/日 |
| ドメイン/時間 | 5000 通/時 |

### 3.2 設定

```yaml
outbound:
  rate_limits:
    per_user:
      hourly: 200
      daily: 1000
    per_domain:
      hourly: 5000
    per_tenant:
      hourly: 10000
```

### 3.3 ホワイトリスト

正当な大量送信用のプロファイル：

```yaml
outbound:
  whitelist:
    - id: "newsletter"
      accounts: ["news@company.com", "marketing@company.com"]
      max_rate_hourly: 5000
      description: "Official newsletter"
    - id: "notifications"
      accounts: ["noreply@company.com"]
      max_rate_hourly: 10000
```

---

## 4. 送信制限レベル

### 4.1 レベル定義

| レベル | 動作 |
|--------|------|
| **Normal** | 通常動作 |
| **Soft Limit** | 送信遅延（キューでレート調整）、管理者通知 |
| **Hard Limit** | 送信完全ブロック、受信は許可 |
| **Domain Limit** | ドメイン全体の送信停止（最悪ケース） |

### 4.2 状態遷移

```
[Normal] ──スコア超過──▶ [Soft Limit]
    ▲                        │
    │ 回復                   │ 継続/悪化
    │                        ▼
    └──────────────── [Hard Limit]
```

### 4.3 設定

```yaml
outbound:
  policies:
    soft_limit:
      threshold_score: 0.5
      threshold_rate: "200 msgs/hour"
    hard_limit:
      threshold_score: 0.8
      threshold_rate: "500 msgs/hour"
    auto_suspend: true
    auto_reenable_after_hours: 0  # 0 = 自動解除なし
```

---

## 5. 実行順序

### 5.1 処理フロー

```
1. Rate Limiter チェック
   └─ 超過 → Soft/Hard Limit 適用

2. Outbound Policy Engine
   └─ スコア計算 → 閾値チェック

3. pre_send Hook（プラグイン）
   └─ DLP、AI スコアリング等

4. 最終判定
   └─ 送信 or ブロック
```

### 5.2 設定による順序変更

特定プラグインを Policy Engine より前に実行：

```yaml
hooks:
  - id: "dlp-check"
    type: "pre_send"
    requires_before_policy: true  # Policy Engine より先に実行
```

---

## 6. 監視サーバー連携

### 6.1 概要

一定の閾値を超えた場合、外部の監視サーバーに通知。

### 6.2 設定

```yaml
outbound:
  spam_monitor:
    enabled: true
    endpoint: "https://monitor.mairust.io/report"  # デフォルト
    api_key: "env:MAIRUST_MONITOR_API_KEY"
    mode: "default"  # default | custom | off
```

**カスタムエンドポイント:**

```yaml
outbound:
  spam_monitor:
    enabled: true
    endpoint: "https://monitor.mycompany.com/mairust"
    api_key: "env:MYCOMPANY_MONITOR_KEY"
    mode: "custom"
```

### 6.3 通知内容

プライバシーを考慮し、本文は送信しない：

```json
{
  "event": "outbound_spam_detected",
  "timestamp": "2024-01-15T10:30:00Z",
  "instance_id": "mairust-001",
  "account_id": "user_abc123",
  "domain": "example.com",
  "metrics": {
    "sent_last_hour": 500,
    "spam_score": 0.85,
    "rules_matched": ["rate_burst", "suspicious_content"]
  },
  "action_taken": "hard_limit"
}
```

### 6.4 通知頻度制限

| 対象 | 制限 |
|------|------|
| 同一アカウント | 最低 10分間隔 |
| 同一アカウント/日 | 最大 24回 |
| システム全体 | 最大 1回/秒 |

### 6.5 通知失敗時

- 送信はブロック **しない**（可用性優先）
- 失敗回数をメトリクス/ログに記録
- 管理者にアラート

---

## 7. 送信停止からの回復

### 7.1 Hard Limit 解除

**デフォルト:** 管理者が手動解除

```bash
# CLI
mairust admin unsuspend-sending --account user@example.com

# API
POST /api/v1/admin/accounts/{id}/unsuspend-sending
```

**オプション:** 自動解除

```yaml
outbound:
  policies:
    auto_reenable_after_hours: 24  # 24時間後に Soft Limit へ移行
```

### 7.2 ユーザーへの通知

#### SMTP エラーメッセージ

```
550 5.7.1 Sending from this account is temporarily suspended. Please contact your administrator.
```

#### Web UI

- バナー表示: 「あなたのアカウントは送信一時停止中です」
- 理由の説明（設定による）

#### 管理者通知

- メール通知（別アドレス宛）
- Webhook（Slack/Teams 等）

---

## 8. 学習データ

### 8.1 保存期間

| データ | 保存期間 |
|--------|----------|
| 詳細送信ログ | 30日 |
| 集計統計 | 180日 |
| アカウント履歴スコア | 365日 |

### 8.2 GDPR 対応

- 保存期間は設定で短縮可能
- 個人データ削除リクエストへの対応

---

## 9. メトリクス

### 9.1 提供メトリクス

| メトリクス | 説明 |
|-----------|------|
| `mairust_outbound_total` | 送信試行総数 |
| `mairust_outbound_blocked_total` | ブロック数 |
| `mairust_outbound_spam_score` | スパムスコア分布 |
| `mairust_outbound_rate_limited_total` | レート制限発動数 |
| `mairust_account_suspended_total` | アカウント停止数 |

### 9.2 アラート設定例

```yaml
alerts:
  - name: "high_spam_rate"
    condition: "rate(mairust_outbound_blocked_total[5m]) > 10"
    severity: "warning"
```

---

## 10. Admin UI

### 10.1 機能

- アカウント別の送信状況ダッシュボード
- スパムスコア履歴グラフ
- 制限中アカウントの一覧
- 手動停止/解除
- ホワイトリスト管理

### 10.2 API

```
GET    /api/v1/admin/outbound/stats
GET    /api/v1/admin/outbound/accounts?status=suspended
POST   /api/v1/admin/outbound/accounts/{id}/suspend
POST   /api/v1/admin/outbound/accounts/{id}/unsuspend
GET    /api/v1/admin/outbound/whitelist
POST   /api/v1/admin/outbound/whitelist
DELETE /api/v1/admin/outbound/whitelist/{id}
```

---

## 関連ドキュメント

- [005-hooks.md](./005-hooks.md) - Hook Manager（pre_send Hook）
- [006-plugins.md](./006-plugins.md) - プラグインアーキテクチャ
- [010-operations.md](./010-operations.md) - 運用設計
