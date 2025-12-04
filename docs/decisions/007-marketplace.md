# 007: マーケットプレイス設計 (Marketplace)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: Backend / Frontend / Infra

---

## 概要

MaiRust プラグインマーケットプレイスの設計を定義します。プラグインの配布、検索、インストール、署名検証、将来の課金モデルについて記載します。

---

## 1. マーケットプレイス概要

### 1.1 目的

- サードパーティ製プラグインの配布・発見を容易にする
- セキュリティ（署名検証）を担保
- 将来的な有料プラグインのエコシステム構築

### 1.2 フェーズ

| Phase | 内容 |
|-------|------|
| Phase 1 | ローカルインストールのみ |
| Phase 2 | 公式 Marketplace（無料プラグイン中心） |
| Phase 3 | 有料プラグイン・課金・ライセンス管理 |
| Phase 4 | 企業向けプライベート Marketplace |

---

## 2. アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                   Marketplace Server                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │  Plugin     │  │  User       │  │  License            │ │
│  │  Registry   │  │  Management │  │  (将来)             │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
           ▲                    ▲
           │ API               │ API
           │                    │
┌──────────┴────────┐  ┌───────┴───────┐
│ MaiRust Instance  │  │ Developer     │
│ (Admin UI)        │  │ Portal        │
└───────────────────┘  └───────────────┘
```

---

## 3. Marketplace API

### 3.1 プラグイン検索

```http
GET /api/plugins?search=spam&mairust_version=0.4.0&category=security

200 OK
{
  "data": [
    {
      "id": "com.example.spam-filter",
      "name": "Example Spam Filter",
      "version": "1.0.0",
      "description": "AI-based spam detection",
      "author": "Example Corp",
      "license": "MIT",
      "category": "security",
      "tags": ["ai", "spam", "filter"],
      "icon_url": "https://marketplace.mairust.io/icons/...",
      "download_count": 1234,
      "rating": 4.5,
      "compat": {
        "mairust_min": "0.3.0",
        "mairust_max": "0.5.x"
      }
    }
  ],
  "pagination": { ... }
}
```

### 3.2 プラグイン詳細

```http
GET /api/plugins/com.example.spam-filter

200 OK
{
  "id": "com.example.spam-filter",
  "name": "Example Spam Filter",
  "versions": [
    { "version": "1.0.0", "released_at": "2024-01-15" },
    { "version": "0.9.0", "released_at": "2024-01-01" }
  ],
  "readme": "# Example Spam Filter\n...",
  "changelog": "## 1.0.0\n- Initial release",
  "permissions": {
    "read_headers": true,
    "read_body": "preview"
  },
  "download_url": "https://marketplace.mairust.io/download/...",
  "checksum": "sha256:..."
}
```

### 3.3 ダウンロード

```http
GET /api/plugins/com.example.spam-filter/download?version=1.0.0

302 Found
Location: https://cdn.mairust.io/plugins/...
```

---

## 4. インストールフロー

### 4.1 Marketplace 経由

```
1. Admin UI で Marketplace を開く
2. プラグインを検索
3. 「インストール」クリック
4. MaiRust が download_url からパッケージ取得
5. checksum 検証
6. 署名検証
7. plugin.toml 解析、互換性チェック
8. 互換 OK → ローカルにインストール（無効状態）
9. 権限レビュー画面を表示
10. 管理者が承認 → 有効化
```

### 4.2 ローカルパッケージ

```bash
# CLI からインストール
mairust plugin install ./my-plugin-1.0.0.mairust-plugin

# API 経由
POST /api/v1/admin/plugins
Content-Type: multipart/form-data
file: <plugin-package>
```

---

## 5. 署名と検証

### 5.1 署名チェーン

```
┌─────────────────────────────────────┐
│ Marketplace Root Key                │
│ (MaiRust プロジェクトが管理)         │
└─────────────────────────────────────┘
           │
           ▼ 署名
┌─────────────────────────────────────┐
│ Plugin Package                      │
│ - manifest.json (ハッシュ一覧)      │
│ - manifest.sig (Ed25519 署名)       │
└─────────────────────────────────────┘
```

### 5.2 manifest.json

```json
{
  "plugin_id": "com.example.spam-filter",
  "version": "1.0.0",
  "files": {
    "plugin.toml": "sha256:abc123...",
    "server/main": "sha256:def456...",
    "README.md": "sha256:ghi789..."
  },
  "signed_at": "2024-01-15T10:30:00Z",
  "developer_signature": "optional-dev-sig..."
}
```

### 5.3 署名アルゴリズム

| 用途 | アルゴリズム |
|------|-------------|
| Marketplace 署名 | Ed25519（必須） |
| 開発者署名 | Ed25519 or RSA-2048（オプション） |

### 5.4 MaiRust 側の検証

1. manifest.json の Marketplace 署名を検証
2. 各ファイルのハッシュを検証
3. 開発者署名があれば表示（信頼度向上の材料）

---

## 6. オフライン環境対応

### 6.1 エクスポート

```bash
# オンライン環境でダウンロード
mairust plugin download com.example.spam-filter --output ./

# => com.example.spam-filter-1.0.0.mairust-plugin
```

### 6.2 オフラインインストール

```bash
# Air-gapped 環境で
mairust plugin install ./com.example.spam-filter-1.0.0.mairust-plugin
```

### 6.3 署名検証

- 署名検証はオフラインでも可能
- Marketplace 公開鍵はMaiRust にバンドル

---

## 7. プラグイン公開

### 7.1 Developer Portal

開発者向けの Web ポータルを提供：

- アカウント登録
- プラグイン登録・更新
- ダウンロード統計
- 収益レポート（将来）

### 7.2 公開フロー

```
1. Developer Portal でアカウント作成
2. プラグインパッケージをアップロード
3. メタデータ入力（説明、カテゴリ、スクリーンショット）
4. 自動静的解析
5. （高権限プラグインの場合）手動レビュー
6. 承認 → Marketplace に公開
```

### 7.3 審査基準

#### 自動チェック

- 必須メタデータの存在
- 互換性情報の妥当性
- 既知の脆弱なライブラリの検出
- パッケージサイズ上限

#### 手動レビュー（高権限）

以下の権限を要求するプラグインは手動レビュー：
- `read_body = "full"`
- `read_attachments = "full"`
- `write_metadata`
- `network = "unlimited"`

---

## 8. セキュリティアラート

### 8.1 悪意プラグイン発覚時

```
1. Marketplace で該当プラグインをブロックリスト登録
2. インストール済みインスタンスに通知 API でアラート送信
3. MaiRust インスタンスは次回同期時に警告表示
4. Admin UI に「このプラグインは危険です」バナー
5. 自動無効化（オプション設定）
```

### 8.2 通知 API

```http
GET /api/security-alerts?since=2024-01-01

{
  "alerts": [
    {
      "plugin_id": "com.malicious.plugin",
      "severity": "critical",
      "message": "This plugin has been identified as malicious",
      "action": "disable_immediately",
      "published_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

---

## 9. プライバシー

### 9.1 MaiRust → Marketplace 送信情報

**必須（匿名）:**
- MaiRust バージョン
- OS / アーキテクチャ

**オプション（明示的オプトイン）:**
- インストール済みプラグイン ID 一覧
- 概算規模（ユーザー数レンジ: 1-10 / 10-100 / 100+）

**絶対に送信しない:**
- ドメイン名
- メールアドレス
- メッセージ内容

### 9.2 設定

```yaml
marketplace:
  enabled: true
  endpoint: "https://marketplace.mairust.io"
  telemetry:
    send_version: true        # 必須
    send_plugins: false       # オプトイン
    send_scale: false         # オプトイン
```

---

## 10. 将来: 課金モデル

### 10.1 収益分配（案）

| 項目 | 比率 |
|------|------|
| 開発者 | 70-80% |
| Marketplace | 20-30% |

### 10.2 ライセンス管理

- 有料プラグインにはライセンスキーを発行
- プラグイン実行時に Marketplace で検証
- オフライン対応: 有効期限付きトークン

### 10.3 Phase 3 以降の検討事項

- 支払い処理（Stripe 等）
- 返金ポリシー
- サブスクリプション vs 買い切り

---

## 11. プライベート Marketplace（Phase 4）

### 11.1 概要

企業向けに自社専用の Marketplace を構築可能。

### 11.2 用途

- 社内プラグインの配布
- 外部 Marketplace との接続制限
- 自社ポリシーに基づく審査

### 11.3 デプロイ

- Docker / Kubernetes でセルフホスト
- 公式 Marketplace のミラー機能

---

## 関連ドキュメント

- [006-plugins.md](./006-plugins.md) - プラグインアーキテクチャ
- [004-authentication.md](./004-authentication.md) - 認証・セキュリティ
- [012-phase1-mvp.md](./012-phase1-mvp.md) - Phase 1 スコープ
