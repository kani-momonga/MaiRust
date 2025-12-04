# Phase 1 実装コードレビュー/品質レビュー

## 読み込んだ資料
- `docs/architecture.md` を含む docs 配下の設計資料
- `development_logs/2025-12-04-phase1-handoff.md` などの開発ログ

## 総評
REST API と spam フィルタ統合の Phase1 実装はエンドポイントが揃っており、ストレージレイヤも整っています。一方で、認証・認可やテナント境界チェックが未実装のまま公開されており、ドキュメントに記載された「マルチテナント管理」「API 鍵認証」の要件を満たせていません。メール送信キュー周りもバリデーションやテナント分離が甘く、実運用前に対処が必要です。

## 主要な懸念点
1. **API キー検証が未実装で保護されていない**
   - `auth_middleware` はヘッダーからキーを抽出するだけで DB 照合や権限チェックをしていません。現状、任意の値を付ければ管理 API を含め全エンドポイントにアクセスできます。早急にキー管理テーブルを作成し、テナントやロールに紐づけた検証を追加すべきです。`/health` 以外を網羅的にカバーするテストも不足しています。【F:crates/mairust-api/src/auth.rs†L15-L53】

2. **テナント分離が守られていないエンドポイントが複数存在**
   - メッセージ取得・フラグ更新・削除は message_id のみで検索しており、`tenant_id` や mailbox の所有者確認をしていません。別テナントのメッセージ ID を知っていれば閲覧・操作できます。【F:crates/mairust-api/src/handlers/messages.rs†L42-L110】
   - Hook の取得/有効化/無効化/削除も hook_id のみで操作し、テナント ID を絞っていません。`list_hooks` も domain と同様に filter がないため、異なるテナントの設定が漏洩します。【F:crates/mairust-api/src/handlers/hooks.rs†L55-L173】【F:crates/mairust-storage/src/repository/hooks.rs†L32-L93】
   - メール送信キュー参照は「tenant_id でフィルタすべき」とコメントされているものの実装されておらず、全テナントのキュー統計・ジョブ詳細を閲覧できます。【F:crates/mairust-api/src/handlers/send.rs†L242-L305】
   - メールボックス作成で指定する `domain_id` / `user_id` が同一テナントか検証しておらず、他テナントのドメイン・ユーザーに紐づくメールボックスを作成できます。`list_by_domain` / `list_by_user` もテナントで絞りません。【F:crates/mairust-api/src/handlers/mailboxes.rs†L41-L128】【F:crates/mairust-storage/src/repository/mailboxes.rs†L12-L99】

3. **ドメイン検証・DKIM 設定がダミーのまま**
   - `verify_domain` は DNS 確認を行わず単に `verified` を true に更新しています。誤設定でも即座に検証済み扱いになるため、送信ドメイン詐称の温床になります。【F:crates/mairust-api/src/handlers/domains.rs†L72-L144】
   - DKIM 設定では秘密鍵の妥当性や selector の重複チェックを行っていません。公開鍵の生成も行わず固定プレースホルダを返すのみで、docs にある DKIM/DMARC の要件を満たしていません。【F:crates/mairust-api/src/handlers/domains.rs†L146-L191】

4. **メール送信キューの入力検証・健全性担保が不足**
   - 添付ファイル content を Base64 として前提にしていますが、実際にはデコードやサイズ上限チェックを行わず、そのままジョブペイロードに保存します。巨大／不正な入力で DB を圧迫したり、キュー処理時に失敗します。【F:crates/mairust-api/src/handlers/send.rs†L13-L225】
   - RFC 準拠のエンコード（Quoted-Printable/Base64）を行わず、本文やヘッダ値をそのまま連結しています。非 ASCII 含みや改行挿入によるヘッダインジェクションの懸念があり、標準ライブラリを使った MIME 構築へ置き換えが必要です。【F:crates/mairust-api/src/handlers/send.rs†L85-L200】

5. **エラーハンドリング・ログが粗く原因追跡が困難**
   - ほぼ全てのハンドラでストレージエラーを `INTERNAL_SERVER_ERROR` へ潰しており、ログ出力もありません。運用時に障害原因が特定できないため、`tracing` でコンテキスト付きログを追加し、クライアントにはエラーコードを返すなどの整備が必要です（例: tenant/domain/user CRUD, message operations など）。【F:crates/mairust-api/src/handlers/tenants.rs†L14-L59】【F:crates/mairust-api/src/handlers/users.rs†L15-L60】【F:crates/mairust-api/src/handlers/domains.rs†L40-L114】

## 改善の優先度
- **P0**: API キー検証実装とテナント境界の一貫した適用（全 CRUD/送信/キュー系）。
- **P1**: ドメイン検証と DKIM キー検証の実装、メール送信 MIME/エンコードの安全化。
- **P2**: 入力サイズ上限・ジョブペイロード制限、詳細なエラー/監査ログの整備、OpenAPI へのセキュリティ要件反映。

## 次のステップ例
- API キー/ロールの DB スキーマ追加と `auth_middleware` の検証実装、ハンドラでのテナント ID 検証ユーティリティ追加。
- Domain/Mailbox/Hook/Message Repository のメソッドを tenant-aware なものに置き換え、既存ハンドラを順次修正する統合テストを追加。
- メール組み立てを `lettre` などの MIME ビルダーへ移行し、添付ファイルの Base64 デコードとサイズチェックを送信前に行う。
