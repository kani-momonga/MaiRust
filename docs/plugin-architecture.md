# MaiRust Plugin Architecture

MaiRust は、メール受信フック・コールバック API をコアとして、  
**AI機能を含むさまざまな拡張機能を「プラグイン」として追加できるアーキテクチャ**を提供します。

このドキュメントでは、プラグインの種類・ライフサイクル・API・パッケージ形式について説明します。

---

## 1. プラグインの目的

- 受信フック（Hooks）を通じて外部ロジックを柔軟に差し込む
- AI機能（分類・要約・スパム判定など）を外部サービス/サードパーティとして実装可能にする
- Web UI や CLI から簡単に追加・削除・更新できるようにする
- 将来的な「サードパーティマーケットプレイス」との統合を可能にする

---

## 2. プラグインの種類

### 2.1 Hook Plugin（メール処理系）

**メールのライフサイクルにフックされるプラグイン。**

紐づくフックタイプ:

- `pre_receive`
- `post_receive`
- `pre_delivery`
- `pre_send`

典型的なユースケース:

- AI スパム判定・分類
- チケットシステム連携
- Slack / Teams 通知
- 独自フィルタリング・タグ付け・ポリシー適用

### 2.2 Service Plugin（バックグラウンド/AIジョブ系）

キューを介してメールID・メタデータを受け取り、非同期で処理し、  
結果をコールバック API で MaiRust に返すプラグイン。

例:

- 大規模な LLM 要約処理
- 高コストの添付ファイル処理（PDF解析など）
- ウイルススキャン連携

### 2.3 UI Plugin（将来的）

Web Admin UI / Web MUA に以下を追加できるような拡張:

- メニュー項目
- サイドバーウィジェット
- メール詳細画面のアクションボタン（「AI要約」「翻訳」「タスク抽出」など）

※ 初期バージョンでは仕様のみ定義し、実装は後フェーズで検討。

---

## 3. プラグインの構造

### 3.1 プラグインパッケージ

プラグインは以下の構造を持つディレクトリ/アーカイブとして扱います:
my-awesome-plugin/
plugin.toml
README.md
icon.png
/server
# バイナリ or スクリプト or Docker イメージ定義
/ui
# （必要なら）フロント拡張


### 3.2 `plugin.toml` 例

```toml
id = "com.example.mairust.ai.spamfilter"
name = "Example AI Spam Filter"
version = "1.0.0"
author = "Example Corp"
description = "AI-based spam detection and classification plugin for MaiRust."
license = "Proprietary"  # or MIT/Apache-2.0 etc.
homepage = "https://example.com/mairust-spamfilter"

[compat]
mairust_min = "0.3.0"
mairust_max = "0.5.x"

[entry]
type = "service"              # hook | service | ui
protocol = "http"             # http | grpc | script
endpoint = "http://localhost:8081/hook"  # or script path
callback_api = true           # MaiRust からの結果受信APIを使用するか

[hooks]
on = ["post_receive"]
filter_recipient_matches = ".*@example.com"

[permissions]
read_headers = true
read_body = "preview"         # none | preview | full
write_tags = true
create_webhook = false
```

## 4. フックとコールバック API の連携
### 4.1 フック側からプラグインサービスへのリクエスト

MaiRust Core → プラグイン（例: HTTP）のリクエスト例:

```http
POST /hook HTTP/1.1
Content-Type: application/json
X-MaiRust-Signature: sha256=...

{
  "hook_type": "post_receive",
  "message_id": "msg_123",
  "mailbox": "user@example.com",
  "envelope": {
    "from": "sender@example.com",
    "to": ["user@example.com"]
  },
  "headers": {
    "Subject": "Hello",
    "Date": "...",
    "Message-Id": "<...>"
  },
  "body": {
    "preview": "Hello, this is a sample...",
    "size": 12345,
    "has_attachments": true
  },
  "metadata": {
    "plugin_id": "com.example.mairust.ai.spamfilter"
  },
  "callback_url": "https://mairust.local/internal/plugins/callback/msg_123"
}
```

### 4.2 プラグインから MaiRust へのコールバック

プラグイン → MaiRust:

```http
POST /internal/plugins/callback/msg_123 HTTP/1.1
Content-Type: application/json
Authorization: Bearer <plugin-token>

{
  "plugin_id": "com.example.mairust.ai.spamfilter",
  "result": {
    "action": "tag",
    "tags": ["ai:spam", "ai:low-priority"],
    "score": 0.98,
    "explanation": "Matched spam pattern and ML score > 0.9"
  }
}
```

MaiRust 側での結果処理例:
- タグ付与
- スレッドへのメタデータ保存
- ログ・メトリクス更新

## 5. ライフサイクル
プラグインには以下のライフサイクルが存在する:

### インストール
- Marketplace から or ローカルパッケージから追加
- plugin.toml を読み取り、DB に登録

### 有効化
- 管理UI/APIから enable
- Hook テーブルに登録

### 実行
- hook で呼び出し・コールバック受信

### 更新
- Marketplace 経由でアップデート（バージョンアップ）
- 互換性チェック（compatセクション）

### 無効化/アンインストール
- Hook から除去
- 設定・ログは必要に応じて保持

## 6. セキュリティ設計

- プラグインごとに 権限スコープ を明示（ヘッダの閲覧、本文、本体アクセス権など）
- コールバック API はプラグイン専用トークンで認証
- Marketplace から署名済みプラグインを配布し、署名検証を行う設計
- ログに「どのプラグインがどのメールに何をしたか」を記録

## 7. 競合との比較（プラグイン観点）
### Haraka（Node.js MTA）
- JSでプラグインを書ける柔軟なアーキテクチャ
- npm 文化と相性が良く、拡張しやすい
- 課題: 型安全性・性能・メール以外も混ざる npm 依存

### Stalwart Mail Server（Rust）
- 高機能なルールパイプラインを持つ
- 内部の設定でかなり柔軟な処理ができる
- 課題: 「サードパーティプラグイン」として配布/管理する仕組みはまだ一般化していない

### Postfix / Exim
- pipe / transport でほぼ何でもできる
- 課題: 設定が難しく、配布・インストール・バージョン管理の仕組みがない

### MaiRust の優位性
- フック + コールバック API を前提にサードパーティプラグインが作りやすい
- マーケットプレイスと連携する前提でメタデータ・権限・互換性情報を標準化
- Rust コアにより、高性能かつ安全な拡張実行基盤を提供
