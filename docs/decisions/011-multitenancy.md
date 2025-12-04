# 011: マルチテナント設計 (Multitenancy)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: Backend / DB

---

## 概要

MaiRust のマルチテナント設計を定義します。テナントの概念、データ分離、権限モデル、リソース制限について記載します。

---

## 1. 用語定義

### 1.1 階層構造

```
Organization（将来）
    └── Tenant
            └── Domain
                    └── User
                            └── Mailbox
```

### 1.2 各概念の定義

| 概念 | 定義 | 例 |
|------|------|-----|
| **Tenant** | 課金・管理の単位（会社・チーム） | "Acme Corp" |
| **Domain** | メールドメイン | "acme.com", "mail.acme.com" |
| **User** | 認証主体（管理者/一般ユーザー） | "admin@acme.com" |
| **Mailbox** | メールの保存先 | "user@acme.com" |
| **Organization** | 複数テナントを束ねる（将来、SaaS 用） | "Enterprise Plan" |

### 1.3 関係

- 1 Tenant : N Domains
- 1 Domain : 1 Tenant（ドメインは共有しない）
- 1 Tenant : N Users
- 1 User : N Mailboxes（エイリアス含む）

---

## 2. データ分離

### 2.1 Phase 1: 論理分離

すべてのテーブルに `tenant_id` カラムを持たせる。

```sql
CREATE TABLE messages (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    mailbox_id UUID NOT NULL,
    subject TEXT,
    ...
);

CREATE INDEX idx_messages_tenant ON messages(tenant_id);
```

### 2.2 クエリのスコープ

すべてのクエリは必ず `tenant_id` でフィルタ：

```rust
// 例: メッセージ取得
fn get_messages(tenant_id: Uuid, mailbox_id: Uuid) -> Vec<Message> {
    db.query("SELECT * FROM messages WHERE tenant_id = $1 AND mailbox_id = $2",
             &[&tenant_id, &mailbox_id])
}
```

### 2.3 将来: 物理分離

エンタープライズ向けに検討：
- テナントごとに DB スキーマ分離
- テナントごとに DB インスタンス分離
- テナントごとに MaiRust インスタンス分離

---

## 3. テナントモデル

### 3.1 テナントテーブル

```sql
CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(63) UNIQUE NOT NULL,  -- URL 用識別子
    status VARCHAR(20) DEFAULT 'active',  -- active, suspended, deleted
    plan VARCHAR(50) DEFAULT 'free',  -- free, pro, enterprise
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);
```

### 3.2 テナント設定

```json
{
  "max_users": 100,
  "max_domains": 10,
  "max_storage_gb": 50,
  "features": {
    "ai_classification": true,
    "custom_plugins": false
  },
  "branding": {
    "logo_url": "https://...",
    "primary_color": "#007bff"
  }
}
```

---

## 4. 権限モデル

### 4.1 ロール

| ロール | 権限 |
|--------|------|
| `super_admin` | 全テナント管理（システム管理者） |
| `tenant_admin` | テナント内の全権限 |
| `domain_admin` | 特定ドメインの管理 |
| `user` | 自身のメールボックスのみ |

### 4.2 権限テーブル

```sql
CREATE TABLE user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    role VARCHAR(50) NOT NULL,
    scope_type VARCHAR(20),  -- tenant, domain, mailbox
    scope_id UUID,  -- domain_id or mailbox_id
    created_at TIMESTAMPTZ DEFAULT now()
);
```

### 4.3 権限チェック例

```rust
fn can_access_mailbox(user: &User, mailbox: &Mailbox) -> bool {
    // super_admin は全アクセス可
    if user.is_super_admin() {
        return true;
    }

    // テナントが一致しない場合は拒否
    if user.tenant_id != mailbox.tenant_id {
        return false;
    }

    // tenant_admin はテナント内全アクセス可
    if user.has_role("tenant_admin", user.tenant_id) {
        return true;
    }

    // 自身のメールボックスのみ
    user.mailbox_ids.contains(&mailbox.id)
}
```

---

## 5. リソース制限

### 5.1 テナント制限

| リソース | Free | Pro | Enterprise |
|----------|------|-----|------------|
| ユーザー数 | 5 | 100 | 無制限 |
| ドメイン数 | 1 | 10 | 無制限 |
| ストレージ | 1GB | 50GB | カスタム |
| 送信数/日 | 100 | 10,000 | カスタム |
| プラグイン | 公式のみ | 全て | 全て + カスタム |

### 5.2 制限チェック

```yaml
# config.yaml
tenants:
  limits:
    free:
      max_users: 5
      max_domains: 1
      max_storage_gb: 1
      max_daily_outbound: 100
    pro:
      max_users: 100
      max_domains: 10
      max_storage_gb: 50
      max_daily_outbound: 10000
```

### 5.3 制限超過時の動作

| リソース | 動作 |
|----------|------|
| ユーザー数超過 | 新規ユーザー作成拒否 |
| ストレージ超過 | 新規メール受信拒否（tempfail） |
| 送信数超過 | 送信拒否 + 管理者通知 |

---

## 6. プラグイン設定

### 6.1 グローバル vs テナント

```
┌─────────────────────────────────────┐
│ Global Plugin Registry              │
│ (プラグインバイナリ・メタデータ)     │
└─────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│ Tenant Plugin Config                │
│ (テナントごとの有効/無効・設定)      │
└─────────────────────────────────────┘
```

### 6.2 テナント別プラグイン設定

```sql
CREATE TABLE tenant_plugins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    plugin_id VARCHAR(255) NOT NULL,
    enabled BOOLEAN DEFAULT false,
    config JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE(tenant_id, plugin_id)
);
```

### 6.3 設定例

```json
{
  "plugin_id": "com.example.spam-filter",
  "enabled": true,
  "config": {
    "sensitivity": "high",
    "whitelist": ["trusted@partner.com"]
  }
}
```

---

## 7. データアクセスパターン

### 7.1 API リクエスト

```
GET /api/v1/messages?tenant_id=xxx
Authorization: Bearer <token>
```

トークンにテナント情報を含む：

```json
{
  "sub": "user_abc123",
  "tenant_id": "tenant_001",
  "roles": ["user"]
}
```

### 7.2 SMTP 受信

```
1. RCPT TO: user@example.com
2. example.com → tenant_001 を解決
3. tenant_001 の設定でフィルタリング
4. tenant_001 のストレージに保存
```

---

## 8. テナント管理 API

### 8.1 エンドポイント

```
# Super Admin 専用
POST   /api/v1/admin/tenants           # テナント作成
GET    /api/v1/admin/tenants           # テナント一覧
GET    /api/v1/admin/tenants/:id       # テナント詳細
PUT    /api/v1/admin/tenants/:id       # テナント更新
DELETE /api/v1/admin/tenants/:id       # テナント削除
POST   /api/v1/admin/tenants/:id/suspend   # 停止
POST   /api/v1/admin/tenants/:id/activate  # 再開

# Tenant Admin
GET    /api/v1/tenant                  # 自テナント情報
PUT    /api/v1/tenant                  # 自テナント更新
GET    /api/v1/tenant/usage            # 使用状況
```

### 8.2 テナント作成

```json
POST /api/v1/admin/tenants
{
  "name": "Acme Corporation",
  "slug": "acme",
  "plan": "pro",
  "admin_email": "admin@acme.com",
  "domains": ["acme.com"]
}
```

---

## 9. テナント削除

### 9.1 削除フロー

```
1. テナント停止（soft delete）
2. 猶予期間（30日）
3. データエクスポート機会の提供
4. 物理削除
   - ユーザー・ドメイン・メールボックス
   - メール本体（ストレージ）
   - プラグイン設定
   - 監査ログは保持（法的要件による）
```

### 9.2 設定

```yaml
tenants:
  deletion:
    grace_period_days: 30
    retain_audit_logs: true
```

---

## 10. ドメイン移管

### 10.1 移管フロー

```
1. 移管元テナント管理者が移管リクエスト
2. 移管先テナント管理者が承認
3. DNS 検証（TXT レコード）
4. ドメイン所有権移転
5. メールボックス移行（オプション）
```

### 10.2 制限

- メールボックスの移行は別途オプション
- 移行中はメール受信停止（短時間）

---

## 11. Phase 1 での実装範囲

### 11.1 実装する

- Tenant テーブル・基本 CRUD
- `tenant_id` による論理分離
- シンプルなロール（admin / user）
- 基本的なリソース制限

### 11.2 Phase 2 以降

- Organization 階層
- 詳細なロール・権限管理
- テナント間ドメイン移管
- 物理分離オプション

---

## 関連ドキュメント

- [003-api.md](./003-api.md) - API 設計
- [004-authentication.md](./004-authentication.md) - 認証・セキュリティ
- [006-plugins.md](./006-plugins.md) - プラグインアーキテクチャ
