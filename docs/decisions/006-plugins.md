# 006: プラグインアーキテクチャ (Plugin Architecture)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: Backend / Plugin SDK

---

## 概要

MaiRust のプラグインシステム設計を定義します。プラグインの種類、パッケージ形式、権限モデル、実行環境、ライフサイクルについて記載します。

---

## 1. プラグインの種類

### 1.1 Hook Plugin

メールのライフサイクルにフックされるプラグイン。

**用途:**
- スパム判定・分類
- チケットシステム連携
- Slack / Teams 通知
- カスタムフィルタリング

### 1.2 Service Plugin

キュー経由で非同期処理を行うプラグイン。

**用途:**
- 大規模 LLM 処理
- PDF 解析
- ウイルススキャン
- バッチ処理

### 1.3 UI Plugin（将来）

Web UI を拡張するプラグイン。

**用途:**
- カスタムパネル
- アクションボタン（「AI要約」「翻訳」等）
- ダッシュボードウィジェット

---

## 2. プラグインパッケージ

### 2.1 ディレクトリ構造

```
my-awesome-plugin/
├── plugin.toml          # メタデータ（必須）
├── README.md            # 説明（推奨）
├── icon.png             # アイコン（推奨、128x128）
├── LICENSE              # ライセンス
├── server/              # サーバーサイド実装
│   ├── main             # バイナリ or スクリプト
│   └── config.yaml      # プラグイン固有設定
└── ui/                  # UI 拡張（将来）
    └── bundle.js
```

### 2.2 パッケージ形式

- 拡張子: `.mairust-plugin`
- 実体: `tar.gz`
- 署名: `manifest.json` + `.sig` ファイル

### 2.3 `plugin.toml` 仕様

```toml
# 基本情報
id = "com.example.mairust.spam-filter"
name = "Example Spam Filter"
version = "1.0.0"
author = "Example Corp"
description = "AI-based spam detection and classification"
license = "MIT"
homepage = "https://example.com/spam-filter"

# 互換性
[compat]
mairust_min = "0.3.0"
mairust_max = "0.5.x"

# エントリポイント
[entry]
type = "hook"                    # hook | service | ui
protocol = "http"                # http | grpc | script
endpoint = "http://localhost:8081/hook"
health_endpoint = "http://localhost:8081/health"
callback_api = true

# 紐づく Hook
[hooks]
on = ["post_receive"]
filter_recipient_matches = ".*@example.com"

# 権限
[permissions]
read_headers = true
read_body = "preview"            # none | preview | full
read_attachments = "metadata"    # none | metadata | preview | full
write_tags = true
write_flags = false
move_message = false
write_metadata = true

# リソース制限
[limits]
cpu_ms = 500
memory_mb = 256
wall_time_ms = 2000
network = "outbound-limited"     # none | local-only | outbound-limited | unlimited
```

---

## 3. 権限モデル

### 3.1 読み取り権限

| 権限 | 説明 |
|------|------|
| `read_headers` | メールヘッダの読み取り |
| `read_body` | メール本文の読み取り |
| `read_attachments` | 添付ファイルの読み取り |

#### `read_body` の値

| 値 | 説明 |
|----|------|
| `none` | 本文アクセス不可 |
| `preview` | 先頭 4KB のみ |
| `full` | 全文アクセス |

#### `read_attachments` の値

| 値 | 説明 |
|----|------|
| `none` | 添付情報なし |
| `metadata` | ファイル名・サイズ・MIME タイプのみ |
| `preview` | 小さいファイル or 先頭数 KB |
| `full` | 全ファイルアクセス |

### 3.2 書き込み権限

| 権限 | 説明 |
|------|------|
| `write_tags` | タグの追加・削除 |
| `write_flags` | フラグ（既読・スター等）の変更 |
| `move_message` | フォルダ移動 |
| `write_metadata` | カスタムメタデータの書き込み |

### 3.3 権限の依存関係

| 権限 | 必要な前提権限 |
|------|---------------|
| `write_tags` | `read_headers` |
| `write_flags` | `read_headers` |
| `move_message` | `read_headers`, `read_body` (preview以上) |
| `write_metadata` | `read_headers` |

### 3.4 権限レビュー

- インストール時に Admin UI で権限一覧を表示
- 高権限（`read_body=full`, `read_attachments=full`）は警告表示
- アップデートで権限追加された場合は再承認が必要

---

## 4. 通信プロトコル

### 4.1 対応プロトコル

| プロトコル | Phase | 用途 |
|-----------|-------|------|
| HTTP(S) | Phase 1 | メイン |
| STDIN/STDOUT | Phase 1 | ローカルスクリプト |
| gRPC | Phase 2 | 高パフォーマンス |
| UNIX socket | Phase 2 | ローカル高速通信 |

### 4.2 HTTP プラグイン

```
POST /hook HTTP/1.1
Content-Type: application/json
X-MaiRust-Signature: sha256=...
X-MaiRust-Timestamp: 1705312800
X-MaiRust-Plugin-Id: com.example.spam-filter

{ ... payload ... }
```

### 4.3 STDIN プラグイン

```bash
# MaiRust がプラグインプロセスを起動
# STDIN に JSON ペイロードを送信
# STDOUT から JSON レスポンスを受信
```

---

## 5. 実行環境

### 5.1 Phase 1: OS ユーザー分離

```
┌─────────────────────────────────────┐
│ MaiRust Core (user: mairust)        │
└─────────────────────────────────────┘
           │
           ▼ fork + setuid
┌─────────────────────────────────────┐
│ Plugin Process (user: mairust-plugin)│
│ - ulimit / cgroup 制限              │
│ - 専用ワークディレクトリ              │
└─────────────────────────────────────┘
```

**制限:**
- ファイルアクセス: `/var/lib/mairust/plugins/<id>` のみ
- メール本体: API 経由でのみアクセス

### 5.2 Phase 3+: コンテナ / WASM（オプション）

- containerd / Docker による完全分離
- WASM (Wasmtime) による軽量サンドボックス

### 5.3 リソース制限

```toml
[limits]
cpu_ms = 500        # CPU 時間上限
memory_mb = 256     # メモリ上限
wall_time_ms = 2000 # 実行時間上限
network = "outbound-limited"
```

#### ネットワーク制限

| 値 | 説明 |
|----|------|
| `none` | ネットワークアクセス禁止 |
| `local-only` | localhost のみ |
| `outbound-limited` | 特定ポート(80/443)・allowlist のみ |
| `unlimited` | 制限なし（要明示許可） |

---

## 6. ヘルスチェック

### 6.1 設定

```toml
[entry]
health_endpoint = "http://localhost:8081/health"
# または
health_cmd = "/opt/plugin/healthcheck.sh"
```

### 6.2 レスポンス形式

```json
{
  "status": "ok",
  "details": "All systems operational"
}
```

| status | 説明 |
|--------|------|
| `ok` | 正常 |
| `degraded` | 一部機能低下 |
| `error` | エラー |

### 6.3 チェック間隔

- デフォルト: 30秒
- 連続失敗時: Circuit Breaker 発動

---

## 7. ログ収集

### 7.1 収集方法

- STDOUT/STDERR をキャプチャ
- ファイル: `/var/log/mairust/plugins/<id>.log`

### 7.2 ログ設定

```toml
[logging]
level = "info"      # trace | debug | info | warn | error
format = "json"     # json | text
```

### 7.3 ログローテーション

- サイズ: 10MB
- 保持: 5世代
- 圧縮: gzip

---

## 8. ライフサイクル

### 8.1 状態遷移

```
[未インストール] → [インストール済み(無効)] → [有効]
                         ↑                      ↓
                         └──── [無効化] ←───────┘
                                  ↓
                          [アンインストール]
```

### 8.2 インストール

1. パッケージをアップロード or Marketplace から取得
2. 署名検証
3. `plugin.toml` 解析、互換性チェック
4. DB に登録（無効状態）
5. ファイルを `/var/lib/mairust/plugins/<id>/` に展開

### 8.3 有効化

1. Admin UI/API から `enable`
2. 権限確認（必要なら承認）
3. Hook テーブルに登録
4. プラグインプロセス起動（Service Plugin の場合）

### 8.4 更新

1. 新バージョンパッケージを取得
2. 互換性チェック
3. 権限変更があれば再承認
4. 旧バージョンをバックアップ
5. 新バージョンに置き換え
6. プラグイン再起動

### 8.5 無効化 / アンインストール

1. Hook から除去
2. プラグインプロセス停止
3. （アンインストールの場合）ファイル削除
4. 設定・ログは保持（オプション）

---

## 9. プラグイン開発

### 9.1 SDK

以下の言語で SDK を提供予定：
- Rust（公式）
- Python
- Go
- Node.js

### 9.2 サンプルプラグイン

```
mairust-plugin-examples/
├── rust-spam-filter/
├── python-ai-classifier/
├── go-webhook-notifier/
└── node-ticket-creator/
```

### 9.3 開発フロー

1. `mairust plugin init` でテンプレート生成
2. ローカルで開発・テスト
3. `mairust plugin package` でパッケージ作成
4. ローカルインストールでテスト
5. Marketplace に公開（オプション）

---

## 10. メトリクス

### 10.1 プラグインメトリクス

| メトリクス | 説明 |
|-----------|------|
| `mairust_plugin_calls_total` | 呼び出し総数 |
| `mairust_plugin_duration_seconds` | 実行時間 |
| `mairust_plugin_errors_total` | エラー数 |
| `mairust_plugin_memory_bytes` | メモリ使用量 |
| `mairust_plugin_network_bytes` | ネットワーク転送量 |

### 10.2 Admin UI での表示

- 呼び出し回数グラフ
- 平均レイテンシ
- エラー率
- リソース使用状況

---

## 11. セキュリティ

### 11.1 署名検証

- Marketplace 署名: Ed25519（必須）
- 開発者署名: Ed25519 or RSA-2048（オプション）

### 11.2 トークン管理

- Plugin Token は DB で暗号化保存
- ローテーション可能
- スコープ制限

### 11.3 監視

- 外部通信の送信先・回数をログ/メトリクスに記録
- Admin UI で確認可能

---

## 関連ドキュメント

- [005-hooks.md](./005-hooks.md) - Hook Manager
- [007-marketplace.md](./007-marketplace.md) - マーケットプレイス
- [004-authentication.md](./004-authentication.md) - 認証・セキュリティ
