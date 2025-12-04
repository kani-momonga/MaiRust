# Phase 1 セキュリティ修正後コードレビュー

## 読み込み資料
- `docs/architecture.md` を含む docs 配下の設計資料
- `development_logs/2025-12-04-phase1-security-fixes.md` の修正内容サマリ

## ポジティブ
- API キー検証が DB バックエンドを持つようになり、プレフィックス検索→ハッシュ照合→期限切れチェック→last_used 更新まで通っている。
- メッセージ/フック/メールボックス送受信系が tenant_id に基づくリポジトリ呼び出しと存在確認を行うようになり、テナント越えの取得・更新・削除が拒否される。
- 送信 API がサイズ・添付数・ヘッダー安全性をチェックし、ジョブキュー参照も payload 内の tenant_id で絞り込むようになった。

## 残っている懸念
1. **ドメイン検証が常に成功する**
   - `perform_dns_verification` は DNS ルックアップを行わないだけでなく、ドメイン名が空でない限り `verified: true` を返してしまう。結果として `/verify` を呼ぶだけで検証済みになり、なりすましドメインでも即座に許可される。【F:crates/mairust-api/src/handlers/domains.rs†L97-L146】【F:crates/mairust-api/src/handlers/domains.rs†L260-L293】
   - 最低でも「DNS 未検証の場合は verified=false を返す」か、TXT/MX を解決する軽量チェックを入れて、誤検証を防ぐ必要がある。

2. **DKIM レコードが実際の鍵と一致しない**
   - `set_dkim` は PEM 形式かどうかは確認するが、公開鍵を抽出せず `p=<YOUR_PUBLIC_KEY>` の固定値を返している。クライアントはこの値をそのまま DNS に設定すると署名検証に失敗するため、保存した秘密鍵から公開鍵を生成してレスポンスに含める処理が必要。【F:crates/mairust-api/src/handlers/domains.rs†L313-L371】【F:crates/mairust-api/src/handlers/domains.rs†L402-L415】

## 推奨アクション
- ドメイン検証: 簡易版でも `trust-dns-resolver` 等で MX/TXT を引いて SPF/DKIM/TXT トークンを確認するか、暫定措置として検証に失敗した場合は verified を更新しないようにする。
- DKIM: `rsa`/`pem` パーサーで公開鍵を抽出し、`p=` にエンコードした鍵を返却する。少なくとも `<YOUR_PUBLIC_KEY>` などのプレースホルダは返さない。
