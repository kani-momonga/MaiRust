//! DKIM (DomainKeys Identified Mail) signing and verification
//!
//! Implements RFC 6376 - DomainKeys Identified Mail (DKIM) Signatures

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rsa::pkcs1v15::{Signature as RsaSignature, SigningKey, VerifyingKey};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::{debug, warn};
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::error::ResolveErrorKind;
use trust_dns_resolver::TokioAsyncResolver;

/// DKIM verification result
#[derive(Debug, Clone, PartialEq)]
pub enum DkimResult {
    /// Signature is valid
    Pass,
    /// Signature verification failed
    Fail,
    /// No signature present
    None,
    /// Temporary error (DNS timeout, etc.)
    TempError,
    /// Permanent error (invalid signature format)
    PermError,
    /// Policy decision (signature not required)
    Policy,
    /// Signature is neutral (valid but not verified)
    Neutral,
}

impl DkimResult {
    /// Convert to header value for Authentication-Results
    pub fn as_header_value(&self) -> &'static str {
        match self {
            DkimResult::Pass => "pass",
            DkimResult::Fail => "fail",
            DkimResult::None => "none",
            DkimResult::TempError => "temperror",
            DkimResult::PermError => "permerror",
            DkimResult::Policy => "policy",
            DkimResult::Neutral => "neutral",
        }
    }
}

/// DKIM canonicalization algorithm
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Canonicalization {
    /// Simple canonicalization
    Simple,
    /// Relaxed canonicalization
    Relaxed,
}

impl Default for Canonicalization {
    fn default() -> Self {
        Canonicalization::Relaxed
    }
}

/// DKIM signing algorithm
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SigningAlgorithm {
    /// RSA-SHA256
    RsaSha256,
    /// Ed25519-SHA256
    Ed25519Sha256,
}

impl Default for SigningAlgorithm {
    fn default() -> Self {
        SigningAlgorithm::RsaSha256
    }
}

/// DKIM signing configuration
#[derive(Debug, Clone)]
pub struct DkimSigningConfig {
    /// Domain name (d= tag)
    pub domain: String,
    /// Selector (s= tag)
    pub selector: String,
    /// Private key (PEM format)
    pub private_key_pem: String,
    /// Signing algorithm
    pub algorithm: SigningAlgorithm,
    /// Header canonicalization
    pub header_canon: Canonicalization,
    /// Body canonicalization
    pub body_canon: Canonicalization,
    /// Headers to sign
    pub headers_to_sign: Vec<String>,
    /// Body length limit (l= tag, None for entire body)
    pub body_length: Option<usize>,
}

impl Default for DkimSigningConfig {
    fn default() -> Self {
        Self {
            domain: String::new(),
            selector: String::new(),
            private_key_pem: String::new(),
            algorithm: SigningAlgorithm::RsaSha256,
            header_canon: Canonicalization::Relaxed,
            body_canon: Canonicalization::Relaxed,
            headers_to_sign: vec![
                "from".to_string(),
                "to".to_string(),
                "subject".to_string(),
                "date".to_string(),
                "message-id".to_string(),
                "mime-version".to_string(),
                "content-type".to_string(),
            ],
            body_length: None,
        }
    }
}

/// DKIM signer for outgoing mail
pub struct DkimSigner {
    config: DkimSigningConfig,
    signing_key: SigningKey<Sha256>,
}

impl DkimSigner {
    /// Create a new DKIM signer
    pub fn new(config: DkimSigningConfig) -> Result<Self> {
        // Parse the RSA private key
        let private_key = parse_rsa_private_key(&config.private_key_pem)?;
        let signing_key = SigningKey::<Sha256>::new(private_key);

        Ok(Self {
            config,
            signing_key,
        })
    }

    /// Create a DKIM signer from an RSA private key directly
    pub fn from_private_key(config: DkimSigningConfig, private_key: RsaPrivateKey) -> Self {
        let signing_key = SigningKey::<Sha256>::new(private_key);
        Self {
            config,
            signing_key,
        }
    }

    /// Sign a message and return the DKIM-Signature header value
    pub fn sign(&self, message: &[u8]) -> Result<String> {
        // Parse message into headers and body
        let (headers, body) = split_message(message)?;

        // Canonicalize body and compute body hash
        let canon_body = self.canonicalize_body(&body);
        let body_hash = compute_sha256_hash(&canon_body);
        let body_hash_b64 = BASE64.encode(&body_hash);

        // Build DKIM-Signature header (without b= value)
        let timestamp = chrono::Utc::now().timestamp();
        let algorithm = match self.config.algorithm {
            SigningAlgorithm::RsaSha256 => "rsa-sha256",
            SigningAlgorithm::Ed25519Sha256 => "ed25519-sha256",
        };
        let canon = format!(
            "{}/{}",
            canon_name(self.config.header_canon),
            canon_name(self.config.body_canon)
        );

        // Get headers to sign (lowercase)
        let signed_headers: Vec<String> = self
            .config
            .headers_to_sign
            .iter()
            .filter(|h| headers.contains_key(&h.to_lowercase()))
            .cloned()
            .collect();

        let mut dkim_header = format!(
            "v=1; a={}; c={}; d={}; s={}; t={}; h={}; bh={}; b=",
            algorithm,
            canon,
            self.config.domain,
            self.config.selector,
            timestamp,
            signed_headers.join(":"),
            body_hash_b64
        );

        // Canonicalize headers for signing
        let canon_headers = self.canonicalize_headers(&headers, &signed_headers, &dkim_header);

        // Sign the canonicalized headers
        let signature = self.signing_key.sign(canon_headers.as_bytes());
        let signature_b64 = BASE64.encode(signature.to_bytes().as_ref());

        // Append signature to header
        dkim_header.push_str(&signature_b64);

        Ok(dkim_header)
    }

    /// Canonicalize message body
    fn canonicalize_body(&self, body: &str) -> Vec<u8> {
        match self.config.body_canon {
            Canonicalization::Simple => {
                // Simple: no change except ensure CRLF line endings
                // and remove trailing empty lines
                let mut result = body.replace('\n', "\r\n");
                while result.ends_with("\r\n\r\n") {
                    result.truncate(result.len() - 2);
                }
                if !result.ends_with("\r\n") {
                    result.push_str("\r\n");
                }
                result.into_bytes()
            }
            Canonicalization::Relaxed => {
                // Relaxed: reduce whitespace, remove trailing whitespace
                let lines: Vec<String> = body
                    .lines()
                    .map(|line| {
                        // Replace sequences of whitespace with single space
                        let mut result = String::new();
                        let mut last_was_space = false;
                        for c in line.chars() {
                            if c.is_whitespace() {
                                if !last_was_space {
                                    result.push(' ');
                                    last_was_space = true;
                                }
                            } else {
                                result.push(c);
                                last_was_space = false;
                            }
                        }
                        // Remove trailing whitespace
                        result.trim_end().to_string()
                    })
                    .collect();

                // Remove trailing empty lines
                let mut lines = lines;
                while lines.last().map_or(false, |l| l.is_empty()) {
                    lines.pop();
                }

                // Join with CRLF and add final CRLF
                let mut result = lines.join("\r\n");
                if !result.is_empty() {
                    result.push_str("\r\n");
                }
                result.into_bytes()
            }
        }
    }

    /// Canonicalize headers for signing
    fn canonicalize_headers(
        &self,
        headers: &HashMap<String, String>,
        signed_headers: &[String],
        dkim_header: &str,
    ) -> String {
        let mut result = String::new();

        for header_name in signed_headers {
            if let Some(value) = headers.get(&header_name.to_lowercase()) {
                match self.config.header_canon {
                    Canonicalization::Simple => {
                        result.push_str(header_name);
                        result.push_str(": ");
                        result.push_str(value);
                        result.push_str("\r\n");
                    }
                    Canonicalization::Relaxed => {
                        result.push_str(&header_name.to_lowercase());
                        result.push(':');
                        // Unfold and reduce whitespace
                        let value = value.replace("\r\n", "").replace('\t', " ");
                        let value: String = value.split_whitespace().collect::<Vec<_>>().join(" ");
                        result.push_str(&value);
                        result.push_str("\r\n");
                    }
                }
            }
        }

        // Add DKIM-Signature header (without trailing CRLF for final hash)
        match self.config.header_canon {
            Canonicalization::Simple => {
                result.push_str("DKIM-Signature: ");
                result.push_str(dkim_header);
            }
            Canonicalization::Relaxed => {
                result.push_str("dkim-signature:");
                let value: String = dkim_header.split_whitespace().collect::<Vec<_>>().join(" ");
                result.push_str(&value);
            }
        }

        result
    }
}

/// DKIM verifier for incoming mail
pub struct DkimVerifier {
    resolver: TokioAsyncResolver,
}

impl DkimVerifier {
    /// Create a new DKIM verifier
    pub async fn new() -> Result<Self> {
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());
        Ok(Self { resolver })
    }

    /// Verify DKIM signature in a message
    pub async fn verify(&self, message: &[u8]) -> DkimResult {
        // Parse message to find DKIM-Signature header
        let (headers, body) = match split_message(message) {
            Ok((h, b)) => (h, b),
            Err(e) => {
                warn!("Failed to parse message for DKIM: {}", e);
                return DkimResult::PermError;
            }
        };

        // Find DKIM-Signature header
        let dkim_sig = match headers.get("dkim-signature") {
            Some(sig) => sig,
            None => {
                debug!("No DKIM-Signature header found");
                return DkimResult::None;
            }
        };

        // Parse DKIM-Signature tags
        let tags = match parse_dkim_tags(dkim_sig) {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to parse DKIM-Signature: {}", e);
                return DkimResult::PermError;
            }
        };

        // Extract required fields
        let domain = match tags.get("d") {
            Some(d) => d,
            None => return DkimResult::PermError,
        };
        let selector = match tags.get("s") {
            Some(s) => s,
            None => return DkimResult::PermError,
        };
        let body_hash = match tags.get("bh") {
            Some(bh) => bh,
            None => return DkimResult::PermError,
        };
        let signed_headers = match tags.get("h") {
            Some(h) => h,
            None => return DkimResult::PermError,
        };
        let signature_b64 = match tags.get("b") {
            Some(b) if !b.is_empty() => b,
            _ => return DkimResult::PermError,
        };
        let algorithm = tags.get("a").map(|s| s.as_str()).unwrap_or("rsa-sha256");
        if !algorithm.eq_ignore_ascii_case("rsa-sha256") {
            warn!("Unsupported DKIM algorithm: {}", algorithm);
            return DkimResult::PermError;
        }

        // Fetch public key from DNS
        let dns_name = format!("{}._domainkey.{}", selector, domain);
        let public_key = match self.fetch_public_key(&dns_name).await {
            Ok(Some(key)) => key,
            Ok(None) => {
                debug!("No DKIM public key found for {}", dns_name);
                return DkimResult::PermError;
            }
            Err(e) => {
                warn!("Failed to fetch DKIM public key: {}", e);
                return DkimResult::TempError;
            }
        };

        // Verify body hash
        let canon = tags.get("c").map(|s| s.as_str()).unwrap_or("simple/simple");
        let (header_canon, body_canon) = parse_canonicalization(canon);

        let computed_body_hash = compute_body_hash(&body, body_canon);
        if computed_body_hash != *body_hash {
            debug!(
                "DKIM body hash mismatch: expected {}, got {}",
                body_hash, computed_body_hash
            );
            return DkimResult::Fail;
        }

        let signed_headers: Vec<String> = signed_headers
            .split(':')
            .map(|h| h.trim().to_lowercase())
            .filter(|h| !h.is_empty())
            .collect();
        if signed_headers.is_empty() {
            return DkimResult::PermError;
        }

        let dkim_header_without_sig = match strip_dkim_signature_value(dkim_sig) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to strip DKIM b= value: {}", e);
                return DkimResult::PermError;
            }
        };

        let canonicalized_headers = canonicalize_headers_for_verification(
            &headers,
            &signed_headers,
            &dkim_header_without_sig,
            header_canon,
        );

        let public_key = match parse_rsa_public_key(&public_key) {
            Ok(key) => key,
            Err(e) => {
                warn!("Failed to parse DKIM public key: {}", e);
                return DkimResult::PermError;
            }
        };

        let signature_bytes = match BASE64.decode(signature_b64) {
            Ok(s) => s,
            Err(e) => {
                warn!("Invalid DKIM signature encoding: {}", e);
                return DkimResult::PermError;
            }
        };
        let signature = match RsaSignature::try_from(signature_bytes.as_slice()) {
            Ok(sig) => sig,
            Err(e) => {
                warn!("Invalid DKIM RSA signature: {}", e);
                return DkimResult::PermError;
            }
        };

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);
        if let Err(e) = verifying_key.verify(canonicalized_headers.as_bytes(), &signature) {
            debug!(
                "DKIM signature verification failed for domain {}: {}",
                domain, e
            );
            return DkimResult::Fail;
        }

        debug!("DKIM signature verified for domain {}", domain);

        DkimResult::Pass
    }

    /// Fetch DKIM public key from DNS
    async fn fetch_public_key(&self, dns_name: &str) -> Result<Option<String>> {
        match self.resolver.txt_lookup(dns_name).await {
            Ok(lookup) => {
                for record in lookup.iter() {
                    let txt = record
                        .txt_data()
                        .iter()
                        .map(|d| String::from_utf8_lossy(d))
                        .collect::<String>();

                    // Parse DKIM record to extract public key
                    if txt.contains("p=") {
                        let tags = parse_dkim_tags(&txt)?;
                        if let Some(key) = tags.get("p") {
                            return Ok(Some(key.clone()));
                        }
                    }
                }
                Ok(None)
            }
            Err(e) => {
                if matches!(e.kind(), ResolveErrorKind::NoRecordsFound { .. }) {
                    Ok(None)
                } else {
                    Err(anyhow!("DNS lookup failed: {}", e))
                }
            }
        }
    }
}

/// Parse RSA private key from PEM format
fn parse_rsa_private_key(pem: &str) -> Result<RsaPrivateKey> {
    use rsa::pkcs8::DecodePrivateKey;
    RsaPrivateKey::from_pkcs8_pem(pem)
        .map_err(|e| anyhow!("Failed to parse RSA private key: {}", e))
}

/// Split message into headers and body
fn split_message(message: &[u8]) -> Result<(HashMap<String, String>, String)> {
    let message_str = String::from_utf8_lossy(message);
    let mut headers = HashMap::new();

    // Find the blank line separating headers from body
    let parts: Vec<&str> = message_str.splitn(2, "\r\n\r\n").collect();
    let (header_section, body) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        // Try with just \n\n
        let parts: Vec<&str> = message_str.splitn(2, "\n\n").collect();
        if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            return Err(anyhow!("Could not find header/body separator"));
        }
    };

    // Parse headers
    let mut current_name = String::new();
    let mut current_value = String::new();

    for line in header_section.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation of previous header
            current_value.push(' ');
            current_value.push_str(line.trim());
        } else if let Some(colon_pos) = line.find(':') {
            // Save previous header
            if !current_name.is_empty() {
                headers.insert(current_name.to_lowercase(), current_value);
            }
            // Start new header
            current_name = line[..colon_pos].to_string();
            current_value = line[colon_pos + 1..].trim().to_string();
        }
    }

    // Save last header
    if !current_name.is_empty() {
        headers.insert(current_name.to_lowercase(), current_value);
    }

    Ok((headers, body.to_string()))
}

/// Parse DKIM tag=value pairs
fn parse_dkim_tags(s: &str) -> Result<HashMap<String, String>> {
    let mut tags = HashMap::new();

    for part in s.split(';') {
        let part = part.trim();
        if let Some(eq_pos) = part.find('=') {
            let name = part[..eq_pos].trim().to_lowercase();
            let value = part[eq_pos + 1..].trim().to_string();
            tags.insert(name, value);
        }
    }

    Ok(tags)
}

/// Compute SHA256 hash
fn compute_sha256_hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Compute body hash for DKIM verification
fn compute_body_hash(body: &str, canon: Canonicalization) -> String {
    let canon_body = match canon {
        Canonicalization::Simple => {
            let mut result = body.replace('\n', "\r\n");
            while result.ends_with("\r\n\r\n") {
                result.truncate(result.len() - 2);
            }
            if !result.ends_with("\r\n") {
                result.push_str("\r\n");
            }
            result.into_bytes()
        }
        Canonicalization::Relaxed => {
            let lines: Vec<String> = body
                .lines()
                .map(|line| {
                    let mut result = String::new();
                    let mut last_was_space = false;
                    for c in line.chars() {
                        if c.is_whitespace() {
                            if !last_was_space {
                                result.push(' ');
                                last_was_space = true;
                            }
                        } else {
                            result.push(c);
                            last_was_space = false;
                        }
                    }
                    result.trim_end().to_string()
                })
                .collect();

            let mut lines = lines;
            while lines.last().map_or(false, |l| l.is_empty()) {
                lines.pop();
            }

            let mut result = lines.join("\r\n");
            if !result.is_empty() {
                result.push_str("\r\n");
            }
            result.into_bytes()
        }
    };

    let hash = compute_sha256_hash(&canon_body);
    BASE64.encode(&hash)
}

fn parse_canonicalization(value: &str) -> (Canonicalization, Canonicalization) {
    let mut parts = value.split('/');
    let header = match parts.next().unwrap_or("simple").trim() {
        "relaxed" => Canonicalization::Relaxed,
        _ => Canonicalization::Simple,
    };
    let body = match parts.next().unwrap_or("simple").trim() {
        "relaxed" => Canonicalization::Relaxed,
        _ => Canonicalization::Simple,
    };
    (header, body)
}

fn strip_dkim_signature_value(dkim_signature_header: &str) -> Result<String> {
    let lower = dkim_signature_header.to_ascii_lowercase();
    let mut search_from = 0usize;

    while let Some(rel_idx) = lower[search_from..].find("b=") {
        let idx = search_from + rel_idx;
        let tag_prefix = &lower[..idx];
        let valid_tag_start = tag_prefix
            .rsplit(';')
            .next()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);

        if valid_tag_start {
            let value_start = idx + 2;
            let value_end = dkim_signature_header[value_start..]
                .find(';')
                .map(|end| value_start + end)
                .unwrap_or(dkim_signature_header.len());

            let mut result = String::with_capacity(dkim_signature_header.len());
            result.push_str(&dkim_signature_header[..value_start]);
            result.push_str(&dkim_signature_header[value_end..]);
            return Ok(result);
        }

        search_from = idx + 2;
    }

    Err(anyhow!("DKIM-Signature header does not contain b= tag"))
}

fn canonicalize_headers_for_verification(
    headers: &HashMap<String, String>,
    signed_headers: &[String],
    dkim_header: &str,
    canonicalization: Canonicalization,
) -> String {
    let mut result = String::new();

    for header_name in signed_headers {
        if let Some(value) = headers.get(header_name) {
            match canonicalization {
                Canonicalization::Simple => {
                    result.push_str(header_name);
                    result.push_str(": ");
                    result.push_str(value);
                    result.push_str("\r\n");
                }
                Canonicalization::Relaxed => {
                    result.push_str(&header_name.to_lowercase());
                    result.push(':');
                    let value = value.replace("\r\n", "").replace('\t', " ");
                    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
                    result.push_str(&value);
                    result.push_str("\r\n");
                }
            }
        }
    }

    match canonicalization {
        Canonicalization::Simple => {
            result.push_str("DKIM-Signature: ");
            result.push_str(dkim_header);
        }
        Canonicalization::Relaxed => {
            result.push_str("dkim-signature:");
            let value = dkim_header.split_whitespace().collect::<Vec<_>>().join(" ");
            result.push_str(&value);
        }
    }

    result
}

fn parse_rsa_public_key(public_key_b64: &str) -> Result<RsaPublicKey> {
    use rsa::pkcs1::DecodeRsaPublicKey;
    use rsa::pkcs8::DecodePublicKey;

    let der = BASE64
        .decode(public_key_b64.trim())
        .map_err(|e| anyhow!("Failed to decode DKIM public key: {}", e))?;

    if let Ok(key) = RsaPublicKey::from_public_key_der(&der) {
        return Ok(key);
    }

    RsaPublicKey::from_pkcs1_der(&der)
        .map_err(|e| anyhow!("Failed to parse DKIM RSA public key DER: {}", e))
}

/// Get canonicalization name
fn canon_name(canon: Canonicalization) -> &'static str {
    match canon {
        Canonicalization::Simple => "simple",
        Canonicalization::Relaxed => "relaxed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dkim_tags() {
        let sig = "v=1; a=rsa-sha256; d=example.com; s=selector1; h=from:to:subject; bh=hash123; b=sig456";
        let tags = parse_dkim_tags(sig).unwrap();

        assert_eq!(tags.get("v"), Some(&"1".to_string()));
        assert_eq!(tags.get("a"), Some(&"rsa-sha256".to_string()));
        assert_eq!(tags.get("d"), Some(&"example.com".to_string()));
        assert_eq!(tags.get("s"), Some(&"selector1".to_string()));
    }

    #[test]
    fn test_split_message() {
        let message = b"From: sender@example.com\r\nTo: recipient@example.com\r\nSubject: Test\r\n\r\nThis is the body.";
        let (headers, body) = split_message(message).unwrap();

        assert_eq!(headers.get("from"), Some(&"sender@example.com".to_string()));
        assert_eq!(
            headers.get("to"),
            Some(&"recipient@example.com".to_string())
        );
        assert_eq!(headers.get("subject"), Some(&"Test".to_string()));
        assert_eq!(body, "This is the body.");
    }

    #[test]
    fn test_dkim_result_header_value() {
        assert_eq!(DkimResult::Pass.as_header_value(), "pass");
        assert_eq!(DkimResult::Fail.as_header_value(), "fail");
        assert_eq!(DkimResult::None.as_header_value(), "none");
    }

    #[test]
    fn test_parse_canonicalization() {
        assert_eq!(
            parse_canonicalization("relaxed/relaxed"),
            (Canonicalization::Relaxed, Canonicalization::Relaxed)
        );
        assert_eq!(
            parse_canonicalization("relaxed"),
            (Canonicalization::Relaxed, Canonicalization::Simple)
        );
    }

    #[test]
    fn test_strip_dkim_signature_value() {
        let header = "v=1; a=rsa-sha256; bh=abc; b=Zm9vYmFy; h=from:to";
        let stripped = strip_dkim_signature_value(header).unwrap();
        assert_eq!(stripped, "v=1; a=rsa-sha256; bh=abc; b=; h=from:to");
    }
}
