# Phase 1 コードレビュー修正 (第2回)

## Date
2025-12-04

## Summary
`development_logs/2025-12-06-phase1-code-review.md` のレビューコメントに従い、ドメイン検証とDKIM公開鍵生成の問題を修正。

## 対処した問題

### 1. ドメイン検証が常に成功する問題
**問題**: `perform_dns_verification` 関数がDNS検証を実際に行わず、ドメイン名が空でない限り `verified: true` を返していた。これにより、なりすましドメインでも即座に検証済みとなる脆弱性があった。

**修正**: `trust-dns-resolver` クレートを使用して実際のDNS検証を実装。

```rust
// 実際のDNS lookupを実行
// 1. MXレコードを確認し、期待されるメールサーバーを指しているかチェック
// 2. TXTレコードを確認し、適切なSPFレコードが設定されているかチェック
// 両方のチェックに合格した場合のみ verified: true を返す
```

### 2. DKIMレコードが実際の鍵と一致しない問題
**問題**: `set_dkim` および `generate_dns_records` 関数が `p=<YOUR_PUBLIC_KEY>` や `p=<public_key>` といった固定値を返しており、実際の公開鍵を生成していなかった。

**修正**: `rsa` クレートを使用して秘密鍵から公開鍵を抽出し、Base64エンコードしてDKIM DNSレコードに含めるように変更。

```rust
// extract_public_key_from_pem 関数を追加
// - PKCS#1/PKCS#8形式のPEM秘密鍵をパース
// - 鍵サイズの検証 (1024-4096 bits)
// - 公開鍵をDER形式でエンコードしBase64に変換
```

## Changes
- `Cargo.toml` (workspace): `trust-dns-resolver` を依存関係に追加
- `crates/mairust-api/Cargo.toml`: `rsa` と `trust-dns-resolver` クレートを依存関係に追加
- `crates/mairust-api/src/handlers/domains.rs`:
  - `perform_dns_verification`: 実際のDNS検証を実装（MXレコードとSPF TXTレコードを確認）
  - `extract_public_key_from_pem`: 新規追加。秘密鍵から公開鍵を抽出
  - `set_dkim`: 公開鍵を実際に抽出してDNSレコードに含める
  - `generate_dns_records`: 保存された秘密鍵から公開鍵を抽出してDKIMレコードに含める

## Technical Details

### DNS検証の実装
`trust-dns-resolver` を使用して実際のDNS lookupを実行:

1. **MXレコード検証**:
   - ドメインのMXレコードをDNS lookupで取得
   - `MAIRUST_HOSTNAME` 環境変数で設定されたメールサーバーを指しているか確認
   - 一致するMXレコードが見つからない場合はエラーを返す

2. **SPFレコード検証**:
   - ドメインのTXTレコードをDNS lookupで取得
   - `v=spf1` で始まるSPFレコードを検索
   - SPFレコードが `mx`、`a:hostname`、`include:hostname` などを含むか確認
   - 適切なSPFレコードが見つからない場合はエラーを返す

3. **検証の条件**:
   - MXレコードとSPFレコードの両方が正しく設定されている場合のみ `verified: true` を返す
   - どちらかが失敗した場合は `verified: false` とエラー詳細を返す

### 公開鍵抽出の実装
1. PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`) とPKCS#8 (`-----BEGIN PRIVATE KEY-----`) の両形式をサポート
2. 鍵サイズを検証 (最小1024ビット、最大4096ビット)
3. 公開鍵をDER形式でエンコードし、Base64に変換

### セキュリティ考慮事項
- DNS検証が未実装の状態で `verified: true` を返すことは、なりすましドメインの許可につながる重大なセキュリティリスク
- 実際のDNS lookupにより、ドメインの所有者がDNSレコードを正しく設定したことを確認

## Test Results
```
test result: ok. 26 passed; 0 failed; 0 ignored
```
