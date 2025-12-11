# Scheduled Email Sending Feature - Design Implementation Report

## Date
2024-12-11

## Summary
スケジュールメール送信機能の設計を完了しました。メールマガジン配信、配信リスト管理、レート制限付き分散配信などの機能を設計し、設計ドキュメントを作成しました。

## Changes
- 新規作成: `docs/decisions/013-scheduled-email.md` - 完全な設計ドキュメント

## Technical Details

### 設計した主要機能

1. **キャンペーン管理**
   - メールマガジンなどの一斉配信単位を管理
   - ドラフト、予約、送信中、完了などの状態管理
   - 送信統計（送信数、配信数、バウンス数、開封数など）

2. **配信リスト**
   - 受信者リストの作成・管理
   - 受信者の状態管理（有効、配信停止、バウンス、苦情）
   - パーソナライズ用カスタム属性

3. **スケジュール配信**
   - 指定日時での送信予約
   - キャンペーンなしの単発スケジュール送信も対応

4. **レート制限と分散配信**
   - テナントごとのレート制限設定（分/時/日）
   - デフォルト: 100通/分、5,000通/時、50,000通/日
   - 大量送信を時間的に分散させるアルゴリズム
   - エラー率に応じた動的レート調整

5. **配信停止（Unsubscribe）機能**
   - 配信停止リンクの自動挿入
   - RFC 8058 One-Click Unsubscribe 対応
   - 配信停止リストによる送信前チェック

### データベース設計

新規テーブル:
- `campaigns` - キャンペーン情報
- `recipient_lists` - 配信リスト
- `recipients` - 受信者
- `scheduled_messages` - スケジュールされた個別メッセージ
- `rate_limit_counters` - レート制限カウンター
- `tenant_rate_limits` - テナントのレート制限設定
- `unsubscribes` - 配信停止リスト

### API設計

主要エンドポイント:
- `POST /campaigns` - キャンペーン作成
- `POST /campaigns/:id/send` - 送信開始
- `POST /campaigns/:id/schedule` - スケジュール設定
- `POST /recipient-lists` - 配信リスト作成
- `POST /scheduled-emails` - 単発スケジュール送信

### アーキテクチャ

```
API Layer
    ↓
Campaign Manager（状態管理、リスト展開、レート計算）
    ↓
Scheduled Job Queue（scheduled_messages テーブル）
    ↓
Delivery Scheduler（5秒間隔ポーリング、レート制限チェック）
```

## Design Decisions

1. **既存のJobsテーブルを使用せず、専用テーブルを新設**
   - 理由: キャンペーン単位の統計やバッチ管理が必要なため

2. **レート制限をテナント単位で設定可能**
   - 理由: プランに応じた柔軟な制限が可能

3. **分散配信アルゴリズム**
   - 1分単位でバッチを作成し、レート制限内で均等に分散

## Test Results
N/A（設計フェーズ）

## Next Steps
1. データベースマイグレーションの実装
2. Campaign Manager の実装
3. Delivery Scheduler の実装
4. API ハンドラーの実装
5. テストの作成
