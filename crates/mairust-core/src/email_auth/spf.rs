//! SPF (Sender Policy Framework) verification
//!
//! Implements RFC 7208 - Sender Policy Framework (SPF) for Authorizing Use of Domains in Email

use anyhow::{anyhow, Result};
use std::net::IpAddr;
use tracing::{debug, warn};
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::TokioAsyncResolver;

/// SPF verification result
#[derive(Debug, Clone, PartialEq)]
pub enum SpfResult {
    /// The sending IP is authorized
    Pass,
    /// The sending IP is explicitly not authorized
    Fail,
    /// The sending IP is probably not authorized (soft fail)
    SoftFail,
    /// The domain owner has no opinion
    Neutral,
    /// No SPF record found
    None,
    /// Temporary error (DNS timeout, etc.)
    TempError,
    /// Permanent error (invalid SPF record)
    PermError,
}

impl SpfResult {
    /// Convert to header value for Authentication-Results
    pub fn as_header_value(&self) -> &'static str {
        match self {
            SpfResult::Pass => "pass",
            SpfResult::Fail => "fail",
            SpfResult::SoftFail => "softfail",
            SpfResult::Neutral => "neutral",
            SpfResult::None => "none",
            SpfResult::TempError => "temperror",
            SpfResult::PermError => "permerror",
        }
    }
}

/// SPF mechanism types
#[derive(Debug, Clone)]
enum SpfMechanism {
    All,
    Include(String),
    A(Option<String>),
    Mx(Option<String>),
    Ip4(ipnet::Ipv4Net),
    Ip6(ipnet::Ipv6Net),
    Ptr(Option<String>),
    Exists(String),
}

/// SPF qualifier (prefix)
#[derive(Debug, Clone, Copy, PartialEq)]
enum SpfQualifier {
    Pass,    // + (default)
    Fail,    // -
    SoftFail, // ~
    Neutral, // ?
}

impl SpfQualifier {
    fn to_result(self) -> SpfResult {
        match self {
            SpfQualifier::Pass => SpfResult::Pass,
            SpfQualifier::Fail => SpfResult::Fail,
            SpfQualifier::SoftFail => SpfResult::SoftFail,
            SpfQualifier::Neutral => SpfResult::Neutral,
        }
    }
}

/// Parsed SPF directive (qualifier + mechanism)
#[derive(Debug, Clone)]
struct SpfDirective {
    qualifier: SpfQualifier,
    mechanism: SpfMechanism,
}

/// SPF verifier
pub struct SpfVerifier {
    resolver: TokioAsyncResolver,
    max_dns_lookups: usize,
}

impl SpfVerifier {
    /// Create a new SPF verifier with default DNS resolver
    pub async fn new() -> Result<Self> {
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());
        Ok(Self {
            resolver,
            max_dns_lookups: 10, // RFC 7208 limit
        })
    }

    /// Create a new SPF verifier with custom resolver
    pub fn with_resolver(resolver: TokioAsyncResolver) -> Self {
        Self {
            resolver,
            max_dns_lookups: 10,
        }
    }

    /// Verify SPF for a given sender and connecting IP
    pub async fn verify(&self, mail_from: &str, client_ip: IpAddr) -> SpfResult {
        // Extract domain from MAIL FROM
        let domain = match extract_domain(mail_from) {
            Some(d) => d,
            None => {
                debug!("Could not extract domain from MAIL FROM: {}", mail_from);
                return SpfResult::None;
            }
        };

        debug!("Checking SPF for domain {} from IP {}", domain, client_ip);

        // Perform SPF check
        match self.check_spf(&domain, client_ip, 0).await {
            Ok(result) => result,
            Err(e) => {
                warn!("SPF check error for {}: {}", domain, e);
                SpfResult::TempError
            }
        }
    }

    /// Recursive SPF check with depth tracking
    fn check_spf<'a>(
        &'a self,
        domain: &'a str,
        client_ip: IpAddr,
        depth: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<SpfResult>> + Send + 'a>> {
        Box::pin(async move {
        if depth >= self.max_dns_lookups {
            return Ok(SpfResult::PermError);
        }

        // Query TXT records for the domain
        let spf_record = match self.get_spf_record(domain).await {
            Ok(Some(record)) => record,
            Ok(None) => return Ok(SpfResult::None),
            Err(e) => {
                warn!("DNS lookup failed for {}: {}", domain, e);
                return Ok(SpfResult::TempError);
            }
        };

        debug!("Found SPF record for {}: {}", domain, spf_record);

        // Parse the SPF record
        let directives = match parse_spf_record(&spf_record) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to parse SPF record for {}: {}", domain, e);
                return Ok(SpfResult::PermError);
            }
        };

        // Evaluate each directive
        for directive in directives {
            match self
                .evaluate_directive(&directive, domain, client_ip, depth)
                .await?
            {
                Some(result) => return Ok(result),
                None => continue,
            }
        }

        // Default result if no mechanism matches
        Ok(SpfResult::Neutral)
        })
    }

    /// Get SPF TXT record for a domain
    async fn get_spf_record(&self, domain: &str) -> Result<Option<String>> {
        let lookup = self.resolver.txt_lookup(domain).await?;

        for record in lookup.iter() {
            let txt = record
                .txt_data()
                .iter()
                .map(|d| String::from_utf8_lossy(d))
                .collect::<String>();

            if txt.starts_with("v=spf1 ") || txt == "v=spf1" {
                return Ok(Some(txt));
            }
        }

        Ok(None)
    }

    /// Evaluate a single SPF directive
    fn evaluate_directive<'a>(
        &'a self,
        directive: &'a SpfDirective,
        domain: &'a str,
        client_ip: IpAddr,
        depth: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<SpfResult>>> + Send + 'a>> {
        Box::pin(async move {
        let matches = match &directive.mechanism {
            SpfMechanism::All => true,

            SpfMechanism::Ip4(network) => {
                if let IpAddr::V4(ip) = client_ip {
                    network.contains(&ip)
                } else {
                    false
                }
            }

            SpfMechanism::Ip6(network) => {
                if let IpAddr::V6(ip) = client_ip {
                    network.contains(&ip)
                } else {
                    false
                }
            }

            SpfMechanism::A(target) => {
                let target_domain = target.as_deref().unwrap_or(domain);
                self.check_a_record(target_domain, client_ip).await?
            }

            SpfMechanism::Mx(target) => {
                let target_domain = target.as_deref().unwrap_or(domain);
                self.check_mx_record(target_domain, client_ip).await?
            }

            SpfMechanism::Include(included_domain) => {
                let result = self
                    .check_spf(included_domain, client_ip, depth + 1)
                    .await?;
                result == SpfResult::Pass
            }

            SpfMechanism::Ptr(_) => {
                // PTR mechanism is deprecated and computationally expensive
                // We treat it as a non-match for security reasons
                warn!("PTR mechanism used but not evaluated (deprecated)");
                false
            }

            SpfMechanism::Exists(macro_domain) => {
                // Check if A record exists for the domain
                self.check_exists(macro_domain).await?
            }
        };

        if matches {
            Ok(Some(directive.qualifier.to_result()))
        } else {
            Ok(None)
        }
        })
    }

    /// Check if client IP matches any A/AAAA record for domain
    async fn check_a_record(&self, domain: &str, client_ip: IpAddr) -> Result<bool> {
        match client_ip {
            IpAddr::V4(ip) => {
                if let Ok(lookup) = self.resolver.ipv4_lookup(domain).await {
                    for record in lookup.iter() {
                        // Convert A record to Ipv4Addr for comparison
                        let record_ip: std::net::Ipv4Addr = (*record).into();
                        if record_ip == ip {
                            return Ok(true);
                        }
                    }
                }
            }
            IpAddr::V6(ip) => {
                if let Ok(lookup) = self.resolver.ipv6_lookup(domain).await {
                    for record in lookup.iter() {
                        // Convert AAAA record to Ipv6Addr for comparison
                        let record_ip: std::net::Ipv6Addr = (*record).into();
                        if record_ip == ip {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    /// Check if client IP matches any MX host's A/AAAA record
    async fn check_mx_record(&self, domain: &str, client_ip: IpAddr) -> Result<bool> {
        if let Ok(mx_lookup) = self.resolver.mx_lookup(domain).await {
            for mx in mx_lookup.iter() {
                let mx_host = mx.exchange().to_string();
                if self.check_a_record(&mx_host, client_ip).await? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Check if A record exists for domain (exists mechanism)
    async fn check_exists(&self, domain: &str) -> Result<bool> {
        Ok(self.resolver.ipv4_lookup(domain).await.is_ok())
    }
}

/// Extract domain from email address
fn extract_domain(email: &str) -> Option<String> {
    // Handle <user@domain> format
    let email = email.trim_start_matches('<').trim_end_matches('>');

    // Find the @ symbol
    if let Some(at_pos) = email.rfind('@') {
        let domain = &email[at_pos + 1..];
        if !domain.is_empty() {
            return Some(domain.to_lowercase());
        }
    }

    None
}

/// Parse SPF record into directives
fn parse_spf_record(record: &str) -> Result<Vec<SpfDirective>> {
    let mut directives = Vec::new();

    // Remove "v=spf1" prefix
    let terms: &str = record
        .strip_prefix("v=spf1")
        .ok_or_else(|| anyhow!("Invalid SPF record: missing v=spf1"))?
        .trim();

    for term in terms.split_whitespace() {
        // Skip modifiers (redirect, exp, etc.) for now
        if term.contains('=') {
            // Handle redirect modifier
            if let Some(domain) = term.strip_prefix("redirect=") {
                directives.push(SpfDirective {
                    qualifier: SpfQualifier::Pass,
                    mechanism: SpfMechanism::Include(domain.to_string()),
                });
            }
            continue;
        }

        // Parse qualifier
        let (qualifier, mechanism_str) = match term.chars().next() {
            Some('+') => (SpfQualifier::Pass, &term[1..]),
            Some('-') => (SpfQualifier::Fail, &term[1..]),
            Some('~') => (SpfQualifier::SoftFail, &term[1..]),
            Some('?') => (SpfQualifier::Neutral, &term[1..]),
            _ => (SpfQualifier::Pass, term),
        };

        // Parse mechanism
        let mechanism = parse_mechanism(mechanism_str)?;

        directives.push(SpfDirective {
            qualifier,
            mechanism,
        });
    }

    Ok(directives)
}

/// Parse a single SPF mechanism
fn parse_mechanism(s: &str) -> Result<SpfMechanism> {
    if s == "all" {
        return Ok(SpfMechanism::All);
    }

    if s == "a" {
        return Ok(SpfMechanism::A(None));
    }

    if let Some(domain) = s.strip_prefix("a:") {
        return Ok(SpfMechanism::A(Some(domain.to_string())));
    }

    if s == "mx" {
        return Ok(SpfMechanism::Mx(None));
    }

    if let Some(domain) = s.strip_prefix("mx:") {
        return Ok(SpfMechanism::Mx(Some(domain.to_string())));
    }

    if let Some(network) = s.strip_prefix("ip4:") {
        let net = if network.contains('/') {
            network.parse()?
        } else {
            format!("{}/32", network).parse()?
        };
        return Ok(SpfMechanism::Ip4(net));
    }

    if let Some(network) = s.strip_prefix("ip6:") {
        let net = if network.contains('/') {
            network.parse()?
        } else {
            format!("{}/128", network).parse()?
        };
        return Ok(SpfMechanism::Ip6(net));
    }

    if let Some(domain) = s.strip_prefix("include:") {
        return Ok(SpfMechanism::Include(domain.to_string()));
    }

    if s == "ptr" {
        return Ok(SpfMechanism::Ptr(None));
    }

    if let Some(domain) = s.strip_prefix("ptr:") {
        return Ok(SpfMechanism::Ptr(Some(domain.to_string())));
    }

    if let Some(domain) = s.strip_prefix("exists:") {
        return Ok(SpfMechanism::Exists(domain.to_string()));
    }

    Err(anyhow!("Unknown SPF mechanism: {}", s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("user@example.com"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_domain("<user@example.com>"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_domain("User@Example.COM"),
            Some("example.com".to_string())
        );
        assert_eq!(extract_domain("nodomain"), None);
        assert_eq!(extract_domain(""), None);
    }

    #[test]
    fn test_parse_spf_record() {
        let record = "v=spf1 ip4:192.168.1.0/24 include:_spf.google.com -all";
        let directives = parse_spf_record(record).unwrap();

        assert_eq!(directives.len(), 3);
        assert!(matches!(
            directives[0].mechanism,
            SpfMechanism::Ip4(_)
        ));
        assert!(matches!(
            directives[1].mechanism,
            SpfMechanism::Include(_)
        ));
        assert!(matches!(directives[2].mechanism, SpfMechanism::All));
        assert_eq!(directives[2].qualifier, SpfQualifier::Fail);
    }

    #[test]
    fn test_spf_result_header_value() {
        assert_eq!(SpfResult::Pass.as_header_value(), "pass");
        assert_eq!(SpfResult::Fail.as_header_value(), "fail");
        assert_eq!(SpfResult::SoftFail.as_header_value(), "softfail");
    }
}
