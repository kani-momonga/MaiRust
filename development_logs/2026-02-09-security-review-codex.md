# 2026-02-09 セキュリティレビュー（SMTP/IMAP/POP3）

## 実施内容
- メールサーバー機能チェック（既存テスト）
- SMTP STARTTLS 実装のコードレビュー
- 設定値の安全性（`require_tls_for_auth` など）確認

## 実行コマンド
- `cargo test -p mairust-core smtp::handler::tests::`

## 主要な確認結果

### 1. STARTTLS の不整合を修正
**問題:**
- 前回変更で `STARTTLS` を全面的に無効化していたため、TLS 設定済みでも SMTP クライアントがセッション昇格できない状態だった。

**対応:**
- `EHLO` で TLS 設定時のみ `STARTTLS` を再広告。
- `STARTTLS` 受信時に、220 応答後に平文 TCP ストリームを再結合し、`rustls` ハンドシェイクを実施。
- TLS 昇格後は同一セッションを継続し、**二重 greeting を送らない**よう修正。
- TLS 確立後の再 `STARTTLS` は `503` 応答。

### 2. 認証保護の確認
- `require_tls_for_auth = true` 時、平文セッションで AUTH を拒否する既存ロジックが有効であることを確認。

## 残課題（次回候補）
- STARTTLS フロー（EHLO→STARTTLS→再EHLO→AUTH）の統合テスト追加。
- IMAP/POP3 側の STARTTLS 経路も同様に統合テストを追加し、回帰を防止。

## 変更ファイル
- `crates/mairust-core/src/smtp/handler.rs`
