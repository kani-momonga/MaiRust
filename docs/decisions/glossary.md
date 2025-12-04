# MaiRust 用語集 (Glossary)

このドキュメントでは、MaiRust プロジェクトで使用される用語を定義します。

---

## A

### API Key
サービス間通信やスクリプトからのAPI呼び出しに使用する長期トークン。スコープ（read-only / admin / plugin 等）を持ち、ローテーション可能。

### Argon2id
パスワードハッシュに使用する推奨アルゴリズム。メモリハード関数であり、ブルートフォース攻撃に強い。

---

## B

### Body Preview
メール本文の先頭部分（デフォルト4KB）のみをプラグインに渡す機能。プライバシー保護とパフォーマンス向上のために使用。

---

## C

### Callback
Plugin → MaiRust に処理結果を返すための内部 HTTP API 呼び出し。Webhook とは方向が逆。

### Circuit Breaker
プラグインの連続失敗時に自動的に呼び出しを停止する仕組み。システム全体の安定性を保護する。

### Core
MaiRust の中心となるコンポーネント。SMTP サーバー、キュー管理、ルーティングエンジン、Hook Manager 等を含む。

---

## D

### Domain
メールドメイン（例: `example.com`）。1つの Tenant に所属し、複数の Domain を持つことができる。

---

## E

### Envelope Encryption
秘密情報（APIキー、プラグインシークレット等）を保存する際の暗号化方式。マスターキーでデータキーを暗号化し、データキーで実データを暗号化する二層構造。

---

## G

### Graceful Shutdown
新規接続の受付を停止し、既存の処理が完了するのを待ってからプロセスを終了する方式。

---

## H

### Hook
MaiRust 内部の処理タイミングで呼ばれるイベントポイント。以下の種類がある：
- `pre_receive`: メール受信前（同期、SMTP セッション内）
- `post_receive`: メール受信後（非同期）
- `pre_send`: メール送信前
- `pre_delivery`: ローカル配送前

### Hook Manager
Hook の登録・実行・結果処理を管理するコンポーネント。タイムアウト制御や Circuit Breaker もここで処理。

### Hook Plugin
メールのライフサイクルにフックされるプラグイン。スパム判定、分類、通知などに使用。

---

## I

### IMAP (Internet Message Access Protocol)
メールボックスへのアクセスプロトコル。Phase 2.5 で read-only 対応、Phase 3 以降でフル対応予定。

---

## J

### JMAP (JSON Meta Application Protocol)
IMAP の代替となるモダンなメールアクセスプロトコル。Phase 3〜4 で検討予定。

---

## L

### Logical Deletion (論理削除)
データを物理的に削除せず、`deleted_at` フラグを設定して非表示にする方式。バックアップや監査のために一定期間保持。

---

## M

### Marketplace
サードパーティが開発したプラグインを配布・販売するプラットフォーム。Phase 2 以降で対応予定。

### Multipart Upload
大きなファイルを分割してアップロードする S3 の機能。デフォルト閾値は 8MB。

---

## O

### Outbound Policy Engine
送信メールのスパム検出・レート制限を行うエンジン。アカウント乗っ取り対策として機能。

### Organization
将来的に複数の Tenant を束ねる概念。SaaS 版で使用予定。Phase 1 では未使用。

---

## P

### Physical Deletion (物理削除)
論理削除から一定期間経過後、実際にデータを削除すること。デフォルトは30日後。

### Plugin
MaiRust の拡張機能の総称。以下の種類がある：
- **Hook Plugin**: メール処理にフックされるプラグイン
- **Service Plugin**: バックグラウンドで動作する非同期処理プラグイン
- **UI Plugin**: Web UI を拡張するプラグイン（将来）
- **Storage Plugin**: ストレージバックエンドを提供するプラグイン（将来）

### Plugin Worker
Plugin のうち、バックグラウンドで動くサービスやジョブ処理プロセスを指す。

### plugin.toml
プラグインのメタデータ・設定を記述するファイル。ID、バージョン、権限、フック定義等を含む。

---

## R

### Rate Limiting
API やメール送信の呼び出し頻度を制限する機能。DoS 攻撃防止やリソース保護に使用。

### rspamd
オープンソースのスパムフィルタリングシステム。Phase 1 で受信スパム検出に連携予定。

---

## S

### Service Plugin
キューを介してメール ID・メタデータを受け取り、非同期で処理するプラグイン。大規模 AI 処理やウイルススキャンなどに使用。

### Soft Limit / Hard Limit
送信スパム対策における制限レベル：
- **Soft Limit**: 送信を遅延させ、管理者に通知
- **Hard Limit**: 送信を完全にブロック

### Storage Backend
メール本体・添付ファイルを保存するストレージの抽象化層。`fs`（ローカル）と `s3`（S3互換）をサポート。

---

## T

### Tenant
組織単位（会社・チームなど）。課金や権限管理の単位。1つの Tenant は複数の Domain を持てる。

### tempfail
SMTP の一時的なエラー応答（4xx）。送信元 MTA は後で再試行する。

---

## U

### UI Plugin
Web Admin UI / Web MUA にメニュー・ウィジェット・アクションボタンを追加するプラグイン。iframe でサンドボックス化される。将来実装予定。

---

## W

### Webhook
MaiRust → 外部サービスへの HTTP 通知。Callback とは方向が逆。

### Whitelist
送信スパム対策において、正当な大量送信（ニュースレター等）を許可するためのリスト。

---

## X

### X-MaiRust-Signature
MaiRust がプラグインへのリクエストに付与する署名ヘッダ。HMAC-SHA256 で計算され、リクエストの正当性を検証するために使用。

---

## 記号・数字

### 4xx (SMTP)
一時的なエラー応答。送信元 MTA は後で再試行すべき。

### 5xx (SMTP)
永続的なエラー応答。送信元 MTA は再試行すべきでない。
