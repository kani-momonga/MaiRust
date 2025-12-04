# 010: 運用設計 (Operations)

**ステータス**: Draft
**最終更新**: 2024-XX-XX
**担当領域**: DevOps / SRE

---

## 概要

MaiRust の運用設計を定義します。監視、バックアップ、障害復旧、アップグレード、スケーリングについて記載します。

---

## 1. 監視

### 1.1 メトリクス

#### Prometheus エンドポイント

```
GET /metrics
```

#### 主要メトリクス

| メトリクス | 説明 |
|-----------|------|
| `mairust_smtp_connections_total` | SMTP 接続数 |
| `mairust_smtp_messages_total` | 処理メッセージ数 |
| `mairust_smtp_duration_seconds` | SMTP トランザクション時間 |
| `mairust_api_requests_total` | API リクエスト数 |
| `mairust_api_duration_seconds` | API レスポンス時間 |
| `mairust_queue_length` | キュー長 |
| `mairust_storage_bytes` | ストレージ使用量 |
| `mairust_hook_calls_total` | Hook 呼び出し数 |
| `mairust_plugin_errors_total` | プラグインエラー数 |

### 1.2 トレーシング

OpenTelemetry 対応：

```yaml
telemetry:
  tracing:
    enabled: true
    exporter: "otlp"
    endpoint: "http://jaeger:4317"
    sample_rate: 0.1
```

### 1.3 アラート設定例

```yaml
# Prometheus AlertManager 用
groups:
  - name: mairust
    rules:
      - alert: HighErrorRate
        expr: rate(mairust_smtp_errors_total[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High SMTP error rate"

      - alert: QueueBacklog
        expr: mairust_queue_length > 1000
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "Mail queue backlog detected"

      - alert: StorageAlmostFull
        expr: mairust_storage_bytes / mairust_storage_limit_bytes > 0.9
        for: 30m
        labels:
          severity: warning
```

### 1.4 ダッシュボード

Grafana ダッシュボードを提供：
- SMTP トラフィック
- API パフォーマンス
- キュー状況
- プラグイン状態
- ストレージ使用率

---

## 2. ログ

### 2.1 ログレベル

| レベル | 用途 |
|--------|------|
| `trace` | 詳細デバッグ |
| `debug` | デバッグ情報 |
| `info` | 通常の動作情報 |
| `warn` | 警告（動作は継続） |
| `error` | エラー |

### 2.2 ログローテーション

```yaml
logging:
  level: "info"
  format: "json"
  rotation:
    max_size_mb: 100
    max_files: 10
    compress: true
```

### 2.3 監査ログ

セキュリティ関連イベントの別ログ：

```yaml
audit:
  enabled: true
  path: "/var/log/mairust/audit.log"
  retention_days: 180
```

---

## 3. バックアップ

### 3.1 バックアップ対象

| 対象 | 方法 |
|------|------|
| PostgreSQL | pg_dump / スナップショット |
| メール本体 (FS) | rsync / スナップショット |
| メール本体 (S3) | S3 バージョニング / クロスリージョン |
| 設定ファイル | バージョン管理 |

### 3.2 バックアップスクリプト例

```bash
#!/bin/bash
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR=/backup/mairust/$DATE

# DB バックアップ
pg_dump -Fc mairust > $BACKUP_DIR/db.dump

# 設定バックアップ
cp -r /etc/mairust $BACKUP_DIR/config

# メール本体（FS の場合）
rsync -av /var/lib/mairust/mail $BACKUP_DIR/mail

# 圧縮
tar czf $BACKUP_DIR.tar.gz $BACKUP_DIR
rm -rf $BACKUP_DIR
```

### 3.3 将来の CLI コマンド

```bash
# フルバックアップ
mairust backup --output /backup/mairust_20240115.tar

# リストア
mairust restore --input /backup/mairust_20240115.tar

# DB のみ
mairust backup --db-only --output /backup/db.dump
```

### 3.4 バックアップスケジュール推奨

| 頻度 | 対象 | 保持期間 |
|------|------|----------|
| 毎時 | DB（WAL） | 24時間 |
| 日次 | DB（フル） | 30日 |
| 日次 | 設定 | 90日 |
| 週次 | メール本体（差分） | 4週間 |

---

## 4. 障害復旧

### 4.1 障害シナリオと対応

#### DB 障害

```
1. DB 接続不可を検知
2. SMTP: tempfail (451) を返す
3. API: 503 を返す
4. アラート発報
5. DB 復旧 or フェイルオーバー
6. サービス自動復旧
```

#### ストレージ障害

```
1. S3/FS アクセス不可を検知
2. 新規メール: tempfail
3. 既存メール閲覧: エラー表示（メタデータは DB から表示可能）
4. アラート発報
5. ストレージ復旧
6. 整合性チェック実行
```

#### 完全復旧手順

```bash
# 1. サービス停止
systemctl stop mairust

# 2. DB リストア
pg_restore -d mairust /backup/db.dump

# 3. メール本体リストア
rsync -av /backup/mail/ /var/lib/mairust/mail/

# 4. 整合性チェック
mairust db check --fix

# 5. サービス起動
systemctl start mairust
```

### 4.2 RTO / RPO 目標

| 指標 | 目標値 |
|------|--------|
| RTO (Recovery Time Objective) | < 1時間 |
| RPO (Recovery Point Objective) | < 1時間（DB） |

---

## 5. アップグレード

### 5.1 アップグレード手順（単一ノード）

```bash
# 1. バックアップ
mairust backup --output /backup/pre-upgrade.tar

# 2. サービス停止
systemctl stop mairust

# 3. バイナリ更新
# (パッケージマネージャー or Docker pull)

# 4. DB マイグレーション
mairust migrate

# 5. サービス起動
systemctl start mairust

# 6. 動作確認
mairust health check
```

### 5.2 ローリングアップデート（複数ノード）

```
1. ノード1を LB から切り離し
2. ノード1をアップグレード
3. ノード1を LB に戻す
4. ノード2以降を順次実行
5. 全ノード完了後、DB マイグレーション（必要な場合）
```

### 5.3 ロールバック

```bash
# バイナリロールバック
# (旧バージョンのパッケージ or Docker イメージ)

# DB ロールバック（必要な場合）
mairust migrate rollback --to=<version>

# 完全ロールバック
mairust restore --input /backup/pre-upgrade.tar
```

### 5.4 API 廃止ポリシー

- 廃止予定エンドポイント: `Deprecation` ヘッダ付与
- 廃止予告期間: 12〜24ヶ月
- 廃止通知: リリースノート、ドキュメント、Admin UI

---

## 6. スケーリング

### 6.1 垂直スケーリング

| コンポーネント | 増強対象 |
|---------------|----------|
| Core | CPU、メモリ |
| API | CPU |
| DB | CPU、メモリ、IOPS |
| Storage | IOPS、容量 |

### 6.2 水平スケーリング

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ Core Node 1 │     │ Core Node 2 │     │ Core Node 3 │
└─────────────┘     └─────────────┘     └─────────────┘
       │                  │                  │
       └──────────────────┼──────────────────┘
                          ▼
                 ┌─────────────────┐
                 │ Shared Storage  │
                 │ (S3 / DB)       │
                 └─────────────────┘
```

### 6.3 スケーリング判断基準

| メトリクス | 閾値 | アクション |
|-----------|------|-----------|
| CPU 使用率 | > 80% (5分) | スケールアウト |
| メモリ使用率 | > 85% | スケールアップ |
| キュー長 | > 10000 | Core スケールアウト |
| API レイテンシ | > 500ms (p95) | API スケールアウト |

---

## 7. Graceful Shutdown

### 7.1 シャットダウンシーケンス

```
1. SIGTERM 受信
2. 新規接続受付停止
3. 既存 SMTP セッション完了待機（最大60秒）
4. キュー処理完了待機（最大30秒）
5. DB 接続クローズ
6. プロセス終了
```

### 7.2 設定

```yaml
server:
  shutdown:
    timeout_seconds: 60
    drain_timeout_seconds: 30
```

---

## 8. ヘルスチェック

### 8.1 エンドポイント

| パス | 説明 |
|------|------|
| `/health` | 基本チェック |
| `/health/live` | Liveness（プロセス生存） |
| `/health/ready` | Readiness（依存サービス含む） |

### 8.2 レスポンス

```json
{
  "status": "healthy",
  "checks": {
    "database": { "status": "healthy", "latency_ms": 5 },
    "storage": { "status": "healthy" },
    "queue": { "status": "healthy", "length": 42 }
  }
}
```

---

## 9. トラブルシューティング

### 9.1 診断コマンド

```bash
# システム状態
mairust status

# 接続テスト
mairust check db
mairust check storage
mairust check smtp

# キュー確認
mairust queue list
mairust queue stats

# ログ確認
mairust logs --tail 100 --component smtp
```

### 9.2 よくある問題

| 症状 | 確認項目 |
|------|----------|
| SMTP 接続拒否 | ポート開放、TLS 証明書 |
| 送信遅延 | キュー長、リモートサーバー応答 |
| API 遅延 | DB 接続数、クエリ性能 |
| ストレージエラー | S3 認証情報、バケット権限 |

---

## 10. SLA / SLO

### 10.1 推奨 SLO

| 指標 | 目標 |
|------|------|
| 可用性 | 99.9%（月間約43分のダウンタイム） |
| SMTP レイテンシ | < 2秒 (p95) |
| API レイテンシ | < 200ms (p95) |
| メール配送遅延 | < 5分（正常時） |

### 10.2 エラーバジェット

- 月間許容ダウンタイム: 43分
- 計画メンテナンス: バジェットから除外可能

---

## 関連ドキュメント

- [002-storage.md](./002-storage.md) - ストレージ設計
- [004-authentication.md](./004-authentication.md) - 認証・セキュリティ
- [009-container.md](./009-container.md) - コンテナ配布
