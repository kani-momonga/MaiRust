# 004: 認証・セキュリティ設計 (Authentication & Security)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: Backend / Security

---

## 概要

MaiRust の認証・認可・セキュリティ設計を定義します。Admin UI、API、SMTP、プラグイン間の認証方式と、データ保護、監査について記載します。

---

## 1. 認証方式の概要

| コンポーネント | 認証方式 |
|---------------|----------|
| Admin UI | セッション Cookie |
| REST API | API Key / Bearer Token |
| SMTP AUTH | PLAIN / LOGIN (TLS上) |
| プラグイン → MaiRust | Plugin Token (Bearer) |
| MaiRust → プラグイン | HMAC 署名 |

---

## 2. Admin UI 認証

### 2.1 方式

**セッション Cookie** ベースの認証を採用。

### 2.2 フロー

```
1. ユーザー: POST /api/v1/auth/login (email, password)
2. サーバー: パスワード検証 → セッション作成 → Set-Cookie
3. ブラウザ: 以降のリクエストに Cookie を自動付与
4. サーバー: セッション検証 → 認可
```

### 2.3 セッション管理

| 項目 | 設定 |
|------|------|
| セッションストア | DB（PostgreSQL） |
| セッション有効期限 | 24時間（設定可能） |
| Cookie 属性 | `HttpOnly`, `Secure`, `SameSite=Lax` |

### 2.4 CSRF 対策

- `SameSite=Lax` で基本的な CSRF を防止
- 追加で CSRF トークンを使用（二重送信 Cookie パターン）

### 2.5 将来: SSO 対応

Phase 2 以降で OIDC / SAML による SSO を検討：
- Auth0
- Keycloak
- Azure AD
- Google Workspace

---

## 3. API 認証

### 3.1 API Key

**用途:** サービス間通信、スクリプト、自動化

**形式:**
```http
Authorization: Bearer mk_live_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

**特徴:**
- 長期有効（明示的に無効化するまで）
- スコープ（権限）を持つ
- ローテーション可能

**スコープ例:**

| スコープ | 説明 |
|---------|------|
| `read` | 読み取りのみ |
| `write` | 読み書き |
| `admin` | 管理操作 |
| `send` | メール送信 |
| `plugin` | プラグイン用 |

### 3.2 短命トークン（Bearer Token）

**用途:** Admin UI からの API 呼び出し

**特徴:**
- ログイン時に発行
- 有効期限: 1時間
- リフレッシュ可能

### 3.3 API Key の管理

```
POST   /api/v1/admin/api-keys         # 作成
GET    /api/v1/admin/api-keys         # 一覧
DELETE /api/v1/admin/api-keys/:id     # 削除（無効化）
POST   /api/v1/admin/api-keys/:id/rotate  # ローテーション
```

---

## 4. SMTP 認証

### 4.1 Phase 1 サポート

| 方式 | 条件 |
|------|------|
| `PLAIN` | TLS/STARTTLS 上のみ |
| `LOGIN` | TLS/STARTTLS 上のみ |

### 4.2 将来対応

| 方式 | Phase |
|------|-------|
| `CRAM-MD5` | Phase 2（レガシー環境向け） |
| `OAUTHBEARER` | Phase 3（OIDC 連携） |

### 4.3 STARTTLS

- **推奨**: STARTTLS を有効化し、認証は暗号化通信上でのみ許可
- 非暗号化での認証はデフォルトで拒否

---

## 5. プラグイン認証

### 5.1 MaiRust → プラグイン

**HMAC 署名** でリクエストの正当性を検証。

**ヘッダ:**
```http
X-MaiRust-Signature: sha256=<signature>
X-MaiRust-Timestamp: 1705312800
X-MaiRust-Plugin-Id: com.example.spam-filter
```

**署名計算:**
```
string_to_sign = timestamp + "\n" + plugin_id + "\n" + sha256(body)
signature = HMAC-SHA256(shared_secret, string_to_sign)
```

**検証:**
- タイムスタンプが ±5分以内であることを確認
- 署名が一致することを確認

### 5.2 プラグイン → MaiRust

**Plugin Token** (Bearer) で認証。

```http
Authorization: Bearer plt_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

**特徴:**
- プラグインごとに発行
- スコープ: `plugin:<id>:callback`, `plugin:<id>:read` 等
- 管理 UI/API から再発行可能

### 5.3 シークレット管理

- 共有シークレット / Plugin Token は DB で暗号化保存
- Envelope Encryption を使用（後述）

---

## 6. パスワード管理

### 6.1 ハッシュアルゴリズム

**Argon2id** を推奨。

**パラメータ（推奨）:**
```
memory: 64 MB
iterations: 3
parallelism: 4
```

### 6.2 パスワードポリシー

| 項目 | デフォルト |
|------|-----------|
| 最小長 | 12文字 |
| 複雑性要件 | なし（長さ優先） |
| 履歴チェック | 過去5個 |

---

## 7. 秘密情報の保護

### 7.1 Envelope Encryption

秘密情報（API Key、プラグインシークレット等）の保存に使用。

```
┌─────────────────────────────────────────────┐
│ Master Key (環境変数 or KMS)                 │
└─────────────────────────────────────────────┘
           │
           ▼ 暗号化
┌─────────────────────────────────────────────┐
│ Data Encryption Key (DEK)                   │
│ (レコードごとに生成)                         │
└─────────────────────────────────────────────┘
           │
           ▼ 暗号化
┌─────────────────────────────────────────────┐
│ 秘密情報（平文）                             │
└─────────────────────────────────────────────┘
```

### 7.2 暗号化アルゴリズム

- **AES-256-GCM** をデフォルト
- 将来: ChaCha20-Poly1305 もオプションとして検討

### 7.3 Master Key 管理

**開発・小規模:**
```yaml
security:
  master_key: "env:MAIRUST_MASTER_KEY"
```

**エンタープライズ:**
- AWS KMS
- HashiCorp Vault
- Azure Key Vault

---

## 8. TLS 設定

### 8.1 バージョン

| 設定 | 値 |
|------|-----|
| 最小バージョン | TLS 1.2 |
| 推奨バージョン | TLS 1.3 |

### 8.2 暗号スイート（TLS 1.2）

推奨：
```
TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
```

無効化：
- RC4
- 3DES
- MD5
- SHA1（署名用）

### 8.3 証明書

```yaml
tls:
  cert_path: "/etc/mairust/certs/server.crt"
  key_path: "/etc/mairust/certs/server.key"
  # オプション: Let's Encrypt 自動更新
  auto_cert:
    enabled: true
    domains: ["mail.example.com"]
```

---

## 9. 監査ログ

### 9.1 対象イベント

| カテゴリ | イベント |
|---------|---------|
| 認証 | ログイン成功/失敗、ログアウト、パスワード変更 |
| 管理 | ユーザー作成/削除、ドメイン設定変更 |
| プラグイン | インストール、有効化/無効化、エラー |
| セキュリティ | API Key 作成/削除、権限変更 |

### 9.2 ログ形式

```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "event_type": "auth.login.success",
  "actor": {
    "type": "user",
    "id": "user_abc123",
    "email": "admin@example.com",
    "ip": "192.168.1.100"
  },
  "target": {
    "type": "session",
    "id": "sess_xyz789"
  },
  "metadata": {
    "user_agent": "Mozilla/5.0..."
  }
}
```

### 9.3 保存期間

- デフォルト: **180日**
- 設定で調整可能

### 9.4 アクセス制限

- 監査ログは admin 権限を持つユーザーのみ閲覧可能
- 改ざん防止のため、別テーブル/別ストレージに保存

---

## 10. 脆弱性対応

### 10.1 報告窓口

- `SECURITY.md` にメールアドレスを記載
- GitHub Security Advisories を使用

### 10.2 対応フロー

```
報告受領 → 調査 → パッチ作成 → リリース → Advisory 公開
    │                                         │
    └──────── 90日以内を目標 ─────────────────┘
```

### 10.3 CVE 採番

- 重大な脆弱性には CVE を取得
- メンテナが CNA 経由で申請

---

## 11. その他のセキュリティ対策

### 11.1 ブルートフォース対策

- ログイン試行: 5回失敗で 15分ロック
- API 認証: レート制限で対応

### 11.2 セッション固定攻撃対策

- ログイン成功時にセッション ID を再生成

### 11.3 XSS 対策

- Content-Security-Policy ヘッダ
- 出力時のエスケープ

### 11.4 SQL インジェクション対策

- プリペアドステートメントの使用
- ORM による自動エスケープ

---

## 12. 管理者ロックアウト時のリカバリ

### 12.1 CLI によるリセット

```bash
mairust admin reset-password --email admin@example.com
```

### 12.2 緊急アクセス

- 環境変数 `MAIRUST_EMERGENCY_TOKEN` で一時的な管理者アクセスを許可
- 使用後は必ず無効化

---

## 関連ドキュメント

- [003-api.md](./003-api.md) - API 設計
- [006-plugins.md](./006-plugins.md) - プラグインアーキテクチャ
- [010-operations.md](./010-operations.md) - 運用設計
