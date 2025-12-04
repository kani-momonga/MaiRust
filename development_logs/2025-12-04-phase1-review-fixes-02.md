# Phase 1 コードレビュー修正 (第2回)

## Date
2025-12-04

## Summary
`development_logs/2025-12-06-phase1-code-review.md` のレビューコメントに従い、ドメイン検証とDKIM公開鍵生成の問題を修正。

## 対処した問題

### 1. ドメイン検証が常に成功する問題
**問題**: `perform_dns_verification` 関数がDNS検証を実際に行わず、ドメイン名が空でない限り `verified: true` を返していた。これにより、なりすましドメインでも即座に検証済みとなる脆弱性があった。

**修正**: 暫定措置として、実際のDNS検証が未実装であることを明確にし、常に `verified: false` を返すように変更。これにより、DNS検証なしにドメインが誤って検証済みになることを防止。

```rust
// 修正前: ドメイン名が空でなければ verified: true
let mx_found = !domain_name.is_empty();
let spf_found = !domain_name.is_empty();

// 修正後: 常に verified: false を返し、実装が必要であることを明示
VerificationStatus {
    verified: false,
    mx_record_found: false,
    spf_record_found: false,
    verification_errors: vec![
        "DNS verification not yet implemented. MX record check required.".to_string(),
        "DNS verification not yet implemented. SPF record check required.".to_string(),
    ],
}
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
- `crates/mairust-api/Cargo.toml`: `rsa` クレートを依存関係に追加
- `crates/mairust-api/src/handlers/domains.rs`:
  - `perform_dns_verification`: 常に `verified: false` を返すように修正
  - `extract_public_key_from_pem`: 新規追加。秘密鍵から公開鍵を抽出
  - `set_dkim`: 公開鍵を実際に抽出してDNSレコードに含める
  - `generate_dns_records`: 保存された秘密鍵から公開鍵を抽出してDKIMレコードに含める

## Technical Details

### 公開鍵抽出の実装
1. PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`) とPKCS#8 (`-----BEGIN PRIVATE KEY-----`) の両形式をサポート
2. 鍵サイズを検証 (最小1024ビット、最大4096ビット)
3. 公開鍵をDER形式でエンコードし、Base64に変換

### セキュリティ考慮事項
- DNS検証が未実装の状態で `verified: true` を返すことは、なりすましドメインの許可につながる重大なセキュリティリスク
- 暫定措置として常に未検証を返すことで、このリスクを排除
- 実際のDNS検証を実装する場合は `trust-dns-resolver` クレートの追加が必要

## Test Results
```
test result: ok. 25 passed; 0 failed; 0 ignored
```

## Next Steps
- `trust-dns-resolver` を使用した実際のDNS検証の実装
  - MXレコードの確認
  - SPF TXTレコードの確認
  - 検証トークンTXTレコードの確認
