# MaiRust Plugin Marketplace

MaiRust は、外部サードパーティが開発したプラグインを  
**マーケットプレイスから購入/ダウンロード/追加**できるエコシステムを目指します。

このドキュメントでは、マーケットプレイスのコンセプト、フロー、セキュリティ、競合比較を記述します。

---

## 1. コンセプト

- AI スパムフィルタ、要約、翻訳、DLP、ウイルススキャンなどを「プラグイン」として提供
- 無料/有料プラグインの共存
- オンプレミス/クラウド問わず使える
- 管理UIからワンクリックでインストール/アップデート可能

---

## 2. マーケットプレイス構成

### 2.1 コンポーネント

- **Marketplace サーバ**
  - プラグインメタデータ・バイナリ/アーカイブのホスティング
  - 課金・ライセンス管理（将来）
- **MaiRust インスタンス**
  - Marketplace に接続してプラグインを検索/インストール
- **Developer Portal**
  - プラグイン開発者向けの登録・公開・更新用ポータル

### 2.2 プラグインメタデータ API（イメージ）

```http
GET /api/plugins?search=ai+spam&mairust_version=0.4.0

200 OK
[
  {
    "id": "com.example.mairust.ai.spamfilter",
    "name": "Example AI Spam Filter",
    "version": "1.0.0",
    "description": "AI-based spam detection",
    "author": "Example Corp",
    "license": "Commercial",
    "price": "10.00",
    "currency": "USD",
    "tags": ["ai", "spam", "filter"],
    "icon_url": "https://...",
    "download_url": "https://.../download/...",
    "checksum": "sha256:..."
  }
]
```

MaiRust 側はこのAPIを叩き、Admin UI でプラグイン一覧を表示する。

## 3. インストールフロー

1. 管理UIでマーケットプレイスを開く
2. プラグインを検索
3. 「インストール」をクリック
4. MaiRust が download_url からプラグインをダウンロード
5. checksum 検証
6. plugin.toml を読み込み、互換性チェック (compat.mairust_min/max)
7. 互換OKならローカルにインストールして無効状態で登録
8. 管理者が「有効化」を行う
9. Hooks と統合され、実行可能になる

※有料プラグインの場合は、Developer Portal / Marketplace 側でライセンスキー発行などを行い、
プラグイン実行時に Marketplace で検証するモデルも想定。

## 4. 更新と削除
### 4.1 更新
- Marketplace は「最新バージョン」「互換バージョン」の情報を提供
- MaiRust は定期的に Marketplace をチェックしアップデート候補を表示
- 管理者が「アップデート」を実行すると、旧バージョンをバックアップしつつ新バージョンに切り替え

### 4.2 削除
- プラグイン無効化
- 設定からフックを削除
- ローカルパッケージを削除（オプションでログ/メタデータは保持）

### 5. セキュリティ
- すべてのプラグインパッケージは署名付き（Marketplace 発行 or 開発者の署名）
- MaiRust はインストール時に署名検証
- プラグインごとに権限スコープ
 - メール本文アクセス
 - ヘッダのみ
 - メタデータのみ
- プラグイン実行は、可能な限りサンドボックスされた環境で実施
 - コンテナ/名前空間/ユーザー分離
- Marketplace には「レビュー/評価/ダウンロード数」などを表示し、信頼性の判断材料を提供

## 6. 競合・他エコシステムとの比較
### Haraka (Node.js)
- npm を通じて多数のプラグインが存在
- 「npm がそのままマーケット」のような位置づけ
- 課題: セキュリティレビューが分散しており、ビジネス的なマーケットプレイスではない

### Gmail / Google Workspace アドオン
- G Suite Marketplace によるアドオンエコシステム
- サードパーティ SaaS と連携しやすい
- 完全に Google 管理のプロプライエタリな世界

### Stalwart Mail Server
- 現時点では「公式マーケットプレイス」のような大規模エコシステムはない
- ルールエンジンや設定でかなり柔軟だが、配布・課金・マーケットまでは整備されていない

### MaiRust のポジション
- 自前インフラ（オンプレ/クラウド）で動かせるメールサーバでありながら、
- Gmail アドオン的なエコシステムを OSS 的な形で提供することを目指す
- 特に AI系プラグインのマーケット にフォーカスすることで、「AI時代のメールプラットフォーム」の地位を狙う

## 7. ロードマップ
- Phase 1: ローカルプラグインインストール（Marketplaceなし）
- Phase 2: シンプルな公式Marketplace（OSSプラグイン中心）
- Phase 3: 有料プラグイン・課金・ライセンス管理
- Phase 4: 企業向けプライベートMarketplace（オンプレRepo）
