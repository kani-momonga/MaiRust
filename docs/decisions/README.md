# MaiRust Design Decisions

このディレクトリには、MaiRust の設計決定事項をまとめたドキュメントが含まれています。
各ドキュメントは機能領域ごとに分割されており、並行して開発を進めやすい構成になっています。

---

## ドキュメント一覧

| ドキュメント | 概要 | 担当領域 |
|-------------|------|----------|
| [glossary.md](./glossary.md) | 用語集 | 全体 |
| [001-foundation.md](./001-foundation.md) | 基本方針（ライセンス、技術選択等） | 全体 |
| [002-storage.md](./002-storage.md) | ストレージ設計（DB、S3、検索エンジン） | Backend |
| [003-api.md](./003-api.md) | REST API 設計 | Backend / Frontend |
| [004-authentication.md](./004-authentication.md) | 認証・セキュリティ設計 | Backend / Security |
| [005-hooks.md](./005-hooks.md) | Hook Manager 設計 | Backend (Core) |
| [006-plugins.md](./006-plugins.md) | プラグインアーキテクチャ | Backend / Plugin SDK |
| [007-marketplace.md](./007-marketplace.md) | マーケットプレイス設計 | Backend / Frontend / Infra |
| [008-outbound-spam.md](./008-outbound-spam.md) | 送信スパム対策 | Backend (Core) |
| [009-container.md](./009-container.md) | コンテナ配布・デプロイ | DevOps / Infra |
| [010-operations.md](./010-operations.md) | 運用設計（監視、バックアップ等） | DevOps / SRE |
| [011-multitenancy.md](./011-multitenancy.md) | マルチテナント設計 | Backend / DB |
| [012-phase1-mvp.md](./012-phase1-mvp.md) | Phase 1 MVP 定義 | PM / 全体 |

---

## ドキュメントの読み方

### 優先度の高い順に読む場合

1. **glossary.md** - 用語の理解
2. **001-foundation.md** - 基本方針の把握
3. **012-phase1-mvp.md** - Phase 1 スコープの確認
4. 担当領域のドキュメント

### 担当領域別

- **Backend Core 開発者**: 002, 005, 008
- **API / Web 開発者**: 003, 004
- **Plugin SDK 開発者**: 005, 006
- **Frontend 開発者**: 003, 007
- **DevOps / SRE**: 009, 010
- **DB / インフラ設計者**: 002, 011

---

## ステータス

- **Draft**: 初稿、レビュー前
- **Review**: レビュー中
- **Approved**: 承認済み、実装可能
- **Implemented**: 実装完了

現在、全ドキュメントは **Draft** ステータスです。

---

## 変更履歴

| 日付 | 変更内容 |
|------|----------|
| 2024-XX-XX | 初版作成 |
