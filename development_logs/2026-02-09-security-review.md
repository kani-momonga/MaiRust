# MaiRust メールサーバー セキュリティレビュー報告書

## Date
2026-02-09

## Summary
MaiRustメールサーバーのコードベース全体に対して、セキュリティ観点から包括的なコードレビューを実施しました。SMTP/IMAP/POP3プロトコル実装、REST API認証・認可、データベース・ファイルストレージ、Webhookシステム、メール認証(SPF/DKIM/DMARC)、ポリシーエンジン、設定管理の各領域を調査しました。

## 調査対象
- `crates/mairust-core/src/smtp/` - SMTP サーバー実装
- `crates/mairust-core/src/imap/` - IMAP サーバー実装
- `crates/mairust-core/src/pop3/` - POP3 サーバー実装
- `crates/mairust-core/src/email_auth/` - SPF/DKIM/DMARC 認証
- `crates/mairust-core/src/hooks/` - Webhook システム
- `crates/mairust-core/src/spam/` - スパムフィルタリング
- `crates/mairust-core/src/policy/` - ポリシーエンジン
- `crates/mairust-api/src/` - REST API 認証・認可
- `crates/mairust-storage/src/` - データベース・ファイルストレージ
- `crates/mairust-common/src/config.rs` - 設定管理

---

## 脆弱性サマリー

| 深刻度 | 件数 |
|--------|------|
| CRITICAL | 16 |
| HIGH | 15 |
| MEDIUM | 19 |
| LOW | 8 |
| **合計** | **58** |

---

## CRITICAL 脆弱性一覧

### 1. STARTTLS未実装 (SMTP)
- **ファイル:** `mairust-core/src/smtp/handler.rs:196-198`
- **問題:** STARTTLSがEHLOでアドバタイズされているが、実際のTLSアップグレードが動作しない。ストリームの所有権がsplitで失われるため、TLSハンドシェイクが不可能。
- **影響:** 暗号化なしで認証情報が送信される可能性。

### 2. DKIM署名検証の未実装
- **ファイル:** `mairust-core/src/email_auth/dkim.rs:404-411`
- **問題:** DKIMはボディハッシュのみ検証し、RSA暗号署名を検証していない。ヘッダーの改ざんを検出できない。
- **影響:** メール送信者のなりすましが可能。DKIM保護が完全に無効化される。

### 3. パストラバーサル (ファイルストレージ)
- **ファイル:** `mairust-storage/src/file.rs:65-66`
- **問題:** `full_path()`がユーザー入力パスを検証なしで結合。`../../../etc/passwd`のようなパスで任意ファイルにアクセス可能。
- **影響:** ファイルシステム上の任意ファイルの読み書き削除。

### 4. テナント分離の欠如 - ドメイン検索
- **ファイル:** `mairust-storage/src/repository/domains.rs:33-39, 77-83`
- **問題:** `get_by_name()`/`find_by_name()`にtenant_idフィルタがない。
- **影響:** テナント間でドメイン情報が漏洩。

### 5. テナント分離の欠如 - ユーザー検索
- **ファイル:** `mairust-storage/src/repository/users.rs:133-139`
- **問題:** `get_by_email()`にtenant_idフィルタがない。
- **影響:** 全テナントのユーザー列挙が可能。

### 6. テナント分離の欠如 - メールボックス検索
- **ファイル:** `mairust-storage/src/repository/mailboxes.rs:35-41, 83-89`
- **問題:** `get_by_address()`にtenant_idフィルタがない。
- **影響:** 全テナントのメールアドレス列挙が可能。

### 7. DKIM秘密鍵の平文保存
- **ファイル:** `mairust-storage/src/repository/domains.rs:106-118`
- **問題:** DKIM秘密鍵がデータベースに平文で保存される。暗号化なし。
- **影響:** DB漏洩時にDKIM秘密鍵が露出。メールなりすましが可能。

### 8. APIキーの弱いハッシュ化
- **ファイル:** `mairust-api/src/auth.rs:79-84`
- **問題:** APIキーがソルトなしSHA256でハッシュ化。高速すぎてブルートフォース攻撃に脆弱。
- **影響:** DB漏洩時にオフラインでAPIキーを復元可能。

### 9. アクセス制御の欠如 - テナントAPI
- **ファイル:** `mairust-api/src/handlers/tenants.rs:15, 29, 45, 60`
- **問題:** テナント管理の全エンドポイントに認証チェックがない。
- **影響:** 任意のユーザーが全テナントの一覧・作成・削除が可能。

### 10. アクセス制御の欠如 - ユーザーAPI
- **ファイル:** `mairust-api/src/handlers/users.rs:15, 30, 46, 61`
- **問題:** ユーザー管理の全エンドポイントに認証チェックがない。
- **影響:** 完全な権限昇格。全テナントのユーザー操作が可能。

### 11. SSRF脆弱性 (Webhook)
- **ファイル:** `mairust-core/src/hooks/manager.rs:277`
- **問題:** WebhookのURLにバリデーションがない。内部ネットワークのサービスにリクエスト送信可能。
- **影響:** 内部サービスへの攻撃、クラウドメタデータ取得、ネットワーク偵察。

### 12. TLSデフォルト無効
- **ファイル:** `mairust-common/src/config.rs:234`
- **問題:** `tls_enabled`のデフォルト値がfalse。
- **影響:** 認証情報やメール本文が平文で送信される。

### 13. 設定ファイルの秘密情報平文保存
- **ファイル:** `mairust-common/src/config.rs:165-168, 354`
- **問題:** S3認証情報、Meilisearch APIキー、rspamdパスワード等が設定ファイルに平文保存。
- **影響:** 設定ファイル漏洩時に全認証情報が露出。

### 14. IMAP COPY/MOVEの権限チェック不足
- **ファイル:** `mairust-core/src/imap/server.rs:1317-1328`
- **問題:** COPY/MOVEコマンドがtenant_idのみチェックし、user_idを検証しない。
- **影響:** 同一テナント内の他ユーザーのメールボックスにメッセージをコピー可能。

### 15. IMAP APPENDのサイズ制限なし
- **ファイル:** `mairust-core/src/imap/server.rs:1595-1702`
- **問題:** APPENDコマンドにメッセージサイズの制限がない。
- **影響:** ディスク容量枯渇によるDoS攻撃。

### 16. IMAP UIDの弱い生成方式
- **ファイル:** `mairust-core/src/imap/server.rs:893-896`
- **問題:** UUIDの最初の4バイトのみ使用。2^32の空間で衝突リスクあり。
- **影響:** メッセージ識別の混乱、誤削除の可能性。

---

## HIGH 脆弱性一覧

### 1. SMTP認証の平文フォールバック
- **ファイル:** `mairust-core/src/smtp/auth.rs:91-101`
- **問題:** AUTH LOGINでbase64デコード失敗時に平文として受け入れる。

### 2. SMTP認証のレート制限なし
- **ファイル:** `mairust-core/src/smtp/auth.rs:108-145`
- **問題:** 認証失敗の回数制限なし。ブルートフォース攻撃が無制限に可能。

### 3. IMAP/POP3のTLS未強制
- **ファイル:** `mairust-core/src/imap/server.rs:31-33`, `pop3/server.rs:30-32`
- **問題:** STARTTLSがオプション。デフォルトで平文ポートを使用。

### 4. POP3メモリ枯渇 (RETR)
- **ファイル:** `mairust-core/src/pop3/server.rs:451-486`
- **問題:** RETRコマンドがメッセージ全体をメモリに読み込み。サイズ制限なし。

### 5. POP3メモリ枯渇 (全メッセージロード)
- **ファイル:** `mairust-core/src/pop3/server.rs:354-362`
- **問題:** PASS認証時に全メッセージをfetch_all()で読み込み。

### 6. APIスコープ検証の欠如
- **ファイル:** 複数のハンドラファイル
- **問題:** 多くのエンドポイントでスコープ検証がない。

### 7. APIレート制限なし
- **ファイル:** `mairust-api/src/routes.rs`
- **問題:** 全エンドポイントにレート制限がない。

### 8. Webhook署名検証の欠如
- **ファイル:** `mairust-core/src/hooks/manager.rs:268-280`
- **問題:** WebhookペイロードにHMAC署名がない。

### 9. SPF redirectメカニズムの誤実装
- **ファイル:** `mairust-core/src/email_auth/spf.rs:345-350`
- **問題:** `redirect=`がIncludeとして処理される。RFC 7208違反。

### 10. ポリシーエンジンのReDoS
- **ファイル:** `mairust-core/src/policy/engine.rs:406-410, 472-477`
- **問題:** ユーザー入力が正規表現として直接コンパイルされる。

### 11. ポリシーリダイレクトURLの未検証
- **ファイル:** `mairust-core/src/policy/engine.rs:782`
- **問題:** リダイレクト先アドレスの検証なし。SSRF類似の脆弱性。

### 12. rspamdパスワードのHTTPヘッダー送信
- **ファイル:** `mairust-core/src/spam/rspamd.rs:173-175`
- **問題:** rspamdパスワードがHTTP Passwordヘッダーで平文送信。

### 13. プレースホルダーパスワードハッシュ
- **ファイル:** `mairust-storage/src/repository/users.rs:52-81`
- **問題:** 簡易create()メソッドが`placeholder_hash`を使用。

### 14. APIキープレフィックスのテナント間漏洩
- **ファイル:** `mairust-storage/src/repository/api_keys.rs:78-90`
- **問題:** `find_by_prefix()`がテナントIDでフィルタしない。

### 15. IMAP AUTHENTICATEの未実装
- **ファイル:** `mairust-core/src/imap/server.rs:227-233`
- **問題:** CAPABILITYでAUTH=PLAINを広告するが未実装。

---

## 修正優先度

### Phase 1: 即時対応 (本番前必須)
1. パストラバーサル修正 (`file.rs:full_path()`)
2. テナント分離の修正 (domains.rs, users.rs, mailboxes.rs)
3. テナント/ユーザーAPIに認証追加 (tenants.rs, users.rs)
4. SSRF防止 (hooks/manager.rs - URL検証)
5. DKIM署名検証の完全実装
6. TLSデフォルト有効化
7. IMAP COPY/MOVEのuser_idチェック追加

### Phase 2: 早急に対応 (1-2週間以内)
1. APIキーハッシュをArgon2に変更
2. DKIM秘密鍵の暗号化保存
3. SMTP認証レート制限の実装
4. SMTP平文フォールバック除去
5. Webhook HMAC署名の実装
6. 秘密情報を環境変数に移行
7. ReDoS防止 (ポリシーエンジン)

### Phase 3: 計画的対応 (1-2ヶ月以内)
1. STARTTLS実装のリファクタリング
2. IMAP/POP3のTLS強制
3. クォータ制限の実装
4. セキュリティヘッダーの追加
5. 監査ログの強化
6. DMARC組織ドメイン検出の改善 (PSL使用)
7. タイミング攻撃対策

---

## 良い実装 (ポジティブ所見)

1. **オープンリレー防止:** SMTP handler.rs:466-489でドメインチェック実施
2. **接続数制限:** semaphoreベースの同時接続制限
3. **メッセージサイズ制限:** SMTPの50MBデフォルト制限
4. **SMTP状態マシン:** コマンド順序の適切な強制
5. **パスワードハッシュ:** argon2使用 (SMTPユーザー認証)
6. **メール形式検証:** parse_mail_from/parse_rcpt_toでの入力検証
7. **SQLパラメータ化:** sqlxのバインドパラメータ使用 (SQLインジェクション防止)
8. **Rustの型安全性:** メモリ安全性はRustの型システムにより保証

## Technical Details

### 調査手法
- 手動コードレビュー (全89個のRustソースファイル)
- セキュリティアンチパターンの静的解析
- OWASP Top 10に基づく脆弱性分類
- メールサーバー固有のセキュリティ基準 (RFC準拠性)

### 対象RFC
- RFC 5321 (SMTP)
- RFC 3501 (IMAP4rev1)
- RFC 1939 (POP3)
- RFC 3207 (SMTP STARTTLS)
- RFC 7208 (SPF)
- RFC 6376 (DKIM)
- RFC 7489 (DMARC)

## Next Steps
1. Phase 1のCRITICAL脆弱性の修正を最優先で実施
2. セキュリティテストスイートの作成
3. CI/CDへのセキュリティスキャン統合
4. 定期的なセキュリティレビューの実施
