//! DMARC (Domain-based Message Authentication, Reporting, and Conformance) verification
//!
//! Implements RFC 7489 - Domain-based Message Authentication, Reporting, and Conformance

use super::dkim::DkimResult;
use super::spf::SpfResult;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use tracing::{debug, warn};
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::error::ResolveErrorKind;
use trust_dns_resolver::TokioAsyncResolver;

/// DMARC policy action
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DmarcPolicy {
    /// No policy (monitor mode)
    None,
    /// Quarantine messages (move to spam)
    Quarantine,
    /// Reject messages
    Reject,
}

impl Default for DmarcPolicy {
    fn default() -> Self {
        DmarcPolicy::None
    }
}

/// DMARC verification result
#[derive(Debug, Clone, PartialEq)]
pub enum DmarcResult {
    /// DMARC check passed
    Pass,
    /// DMARC check failed with specified policy
    Fail(DmarcPolicy),
    /// No DMARC record found
    None,
    /// Temporary error
    TempError,
    /// Permanent error
    PermError,
}

impl DmarcResult {
    /// Convert to header value for Authentication-Results
    pub fn as_header_value(&self) -> &'static str {
        match self {
            DmarcResult::Pass => "pass",
            DmarcResult::Fail(_) => "fail",
            DmarcResult::None => "none",
            DmarcResult::TempError => "temperror",
            DmarcResult::PermError => "permerror",
        }
    }

    /// Get the policy for failed DMARC
    pub fn policy(&self) -> Option<DmarcPolicy> {
        match self {
            DmarcResult::Fail(policy) => Some(*policy),
            _ => None,
        }
    }
}

/// Parsed DMARC record
#[derive(Debug, Clone)]
pub struct DmarcRecord {
    /// Policy for messages from the domain (p=)
    pub policy: DmarcPolicy,
    /// Policy for subdomains (sp=)
    pub subdomain_policy: Option<DmarcPolicy>,
    /// Percentage of messages to apply policy (pct=)
    pub percentage: u8,
    /// DKIM alignment mode (adkim=)
    pub dkim_alignment: AlignmentMode,
    /// SPF alignment mode (aspf=)
    pub spf_alignment: AlignmentMode,
    /// Aggregate report URI (rua=)
    pub aggregate_report_uri: Option<String>,
    /// Forensic report URI (ruf=)
    pub forensic_report_uri: Option<String>,
    /// Failure reporting options (fo=)
    pub failure_options: String,
    /// Report format (rf=)
    pub report_format: String,
    /// Report interval in seconds (ri=)
    pub report_interval: u32,
}

impl Default for DmarcRecord {
    fn default() -> Self {
        Self {
            policy: DmarcPolicy::None,
            subdomain_policy: None,
            percentage: 100,
            dkim_alignment: AlignmentMode::Relaxed,
            spf_alignment: AlignmentMode::Relaxed,
            aggregate_report_uri: None,
            forensic_report_uri: None,
            failure_options: "0".to_string(),
            report_format: "afrf".to_string(),
            report_interval: 86400,
        }
    }
}

/// Alignment mode for DKIM/SPF
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignmentMode {
    /// Strict: domains must match exactly
    Strict,
    /// Relaxed: organizational domains must match
    Relaxed,
}

impl Default for AlignmentMode {
    fn default() -> Self {
        AlignmentMode::Relaxed
    }
}

/// DMARC verifier
pub struct DmarcVerifier {
    resolver: TokioAsyncResolver,
}

impl DmarcVerifier {
    /// Create a new DMARC verifier
    pub async fn new() -> Result<Self> {
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());
        Ok(Self { resolver })
    }

    /// Verify DMARC for a message
    ///
    /// # Arguments
    /// * `from_domain` - Domain from the From header
    /// * `mail_from_domain` - Domain from MAIL FROM (envelope)
    /// * `dkim_domain` - Domain from DKIM signature (d= tag)
    /// * `spf_result` - Result of SPF check
    /// * `dkim_result` - Result of DKIM check
    pub async fn verify(
        &self,
        from_domain: &str,
        mail_from_domain: Option<&str>,
        dkim_domain: Option<&str>,
        spf_result: &SpfResult,
        dkim_result: &DkimResult,
    ) -> DmarcResult {
        // Fetch DMARC record
        let dmarc_record = match self.fetch_dmarc_record(from_domain).await {
            Ok(Some(record)) => record,
            Ok(None) => {
                debug!("No DMARC record found for {}", from_domain);
                return DmarcResult::None;
            }
            Err(e) => {
                warn!("Failed to fetch DMARC record: {}", e);
                return DmarcResult::TempError;
            }
        };

        debug!("Found DMARC record for {}: {:?}", from_domain, dmarc_record);

        // Check SPF alignment
        let spf_aligned = if *spf_result == SpfResult::Pass {
            if let Some(mail_from) = mail_from_domain {
                check_alignment(from_domain, mail_from, dmarc_record.spf_alignment)
            } else {
                false
            }
        } else {
            false
        };

        // Check DKIM alignment
        let dkim_aligned = if *dkim_result == DkimResult::Pass {
            if let Some(dkim_d) = dkim_domain {
                check_alignment(from_domain, dkim_d, dmarc_record.dkim_alignment)
            } else {
                false
            }
        } else {
            false
        };

        // DMARC passes if either SPF or DKIM is aligned
        if spf_aligned || dkim_aligned {
            debug!(
                "DMARC pass for {}: SPF aligned={}, DKIM aligned={}",
                from_domain, spf_aligned, dkim_aligned
            );
            DmarcResult::Pass
        } else {
            debug!(
                "DMARC fail for {}: SPF aligned={}, DKIM aligned={}, policy={:?}",
                from_domain, spf_aligned, dkim_aligned, dmarc_record.policy
            );
            DmarcResult::Fail(dmarc_record.policy)
        }
    }

    /// Fetch DMARC record from DNS
    fn fetch_dmarc_record<'a>(
        &'a self,
        domain: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<DmarcRecord>>> + Send + 'a>> {
        Box::pin(async move {
            let dmarc_domain = format!("_dmarc.{}", domain);

            match self.resolver.txt_lookup(&dmarc_domain).await {
                Ok(lookup) => {
                    for record in lookup.iter() {
                        let txt = record
                            .txt_data()
                            .iter()
                            .map(|d| String::from_utf8_lossy(d))
                            .collect::<String>();

                        if txt.starts_with("v=DMARC1") {
                            return Ok(Some(parse_dmarc_record(&txt)?));
                        }
                    }
                    // No DMARC record found, try organizational domain
                    if let Some(org_domain) = get_organizational_domain(domain) {
                        if org_domain != domain {
                            return self.fetch_dmarc_record(&org_domain).await;
                        }
                    }
                    Ok(None)
                }
                Err(e) => {
                    if matches!(e.kind(), ResolveErrorKind::NoRecordsFound { .. }) {
                        // Try organizational domain
                        if let Some(org_domain) = get_organizational_domain(domain) {
                            if org_domain != domain {
                                return self.fetch_dmarc_record(&org_domain).await;
                            }
                        }
                        Ok(None)
                    } else {
                        Err(anyhow!("DNS lookup failed: {}", e))
                    }
                }
            }
        })
    }
}

/// Parse DMARC record from TXT value
fn parse_dmarc_record(txt: &str) -> Result<DmarcRecord> {
    let mut record = DmarcRecord::default();

    // Parse tags
    let tags = parse_tags(txt)?;

    // Version (required)
    if tags.get("v") != Some(&"DMARC1".to_string()) {
        return Err(anyhow!("Invalid DMARC version"));
    }

    // Policy (required)
    if let Some(p) = tags.get("p") {
        record.policy = parse_policy(p)?;
    } else {
        return Err(anyhow!("Missing required p= tag"));
    }

    // Subdomain policy
    if let Some(sp) = tags.get("sp") {
        record.subdomain_policy = Some(parse_policy(sp)?);
    }

    // Percentage
    if let Some(pct) = tags.get("pct") {
        record.percentage = pct
            .parse()
            .map_err(|_| anyhow!("Invalid pct value: {}", pct))?;
    }

    // DKIM alignment
    if let Some(adkim) = tags.get("adkim") {
        record.dkim_alignment = parse_alignment(adkim)?;
    }

    // SPF alignment
    if let Some(aspf) = tags.get("aspf") {
        record.spf_alignment = parse_alignment(aspf)?;
    }

    // Report URIs
    if let Some(rua) = tags.get("rua") {
        record.aggregate_report_uri = Some(rua.clone());
    }
    if let Some(ruf) = tags.get("ruf") {
        record.forensic_report_uri = Some(ruf.clone());
    }

    // Failure options
    if let Some(fo) = tags.get("fo") {
        record.failure_options = fo.clone();
    }

    // Report format
    if let Some(rf) = tags.get("rf") {
        record.report_format = rf.clone();
    }

    // Report interval
    if let Some(ri) = tags.get("ri") {
        record.report_interval = ri
            .parse()
            .map_err(|_| anyhow!("Invalid ri value: {}", ri))?;
    }

    Ok(record)
}

/// Parse DMARC tags
fn parse_tags(txt: &str) -> Result<HashMap<String, String>> {
    let mut tags = HashMap::new();

    for part in txt.split(';') {
        let part = part.trim();
        if let Some(eq_pos) = part.find('=') {
            let name = part[..eq_pos].trim().to_lowercase();
            let value = part[eq_pos + 1..].trim().to_string();
            tags.insert(name, value);
        }
    }

    Ok(tags)
}

/// Parse DMARC policy
fn parse_policy(s: &str) -> Result<DmarcPolicy> {
    match s.to_lowercase().as_str() {
        "none" => Ok(DmarcPolicy::None),
        "quarantine" => Ok(DmarcPolicy::Quarantine),
        "reject" => Ok(DmarcPolicy::Reject),
        _ => Err(anyhow!("Invalid policy: {}", s)),
    }
}

/// Parse alignment mode
fn parse_alignment(s: &str) -> Result<AlignmentMode> {
    match s.to_lowercase().as_str() {
        "r" => Ok(AlignmentMode::Relaxed),
        "s" => Ok(AlignmentMode::Strict),
        _ => Err(anyhow!("Invalid alignment mode: {}", s)),
    }
}

/// Check if two domains are aligned
fn check_alignment(from_domain: &str, auth_domain: &str, mode: AlignmentMode) -> bool {
    let from_domain = from_domain.to_lowercase();
    let auth_domain = auth_domain.to_lowercase();

    match mode {
        AlignmentMode::Strict => from_domain == auth_domain,
        AlignmentMode::Relaxed => {
            // Get organizational domains and compare
            let from_org = get_organizational_domain(&from_domain)
                .unwrap_or_else(|| from_domain.clone());
            let auth_org = get_organizational_domain(&auth_domain)
                .unwrap_or_else(|| auth_domain.clone());
            from_org == auth_org
        }
    }
}

/// Get organizational domain (simplified - in production use PSL)
fn get_organizational_domain(domain: &str) -> Option<String> {
    let parts: Vec<&str> = domain.split('.').collect();
    if parts.len() >= 2 {
        // Simple heuristic: last two parts
        // In production, use the Public Suffix List
        Some(parts[parts.len() - 2..].join("."))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dmarc_record() {
        let txt = "v=DMARC1; p=reject; sp=quarantine; pct=50; adkim=s; aspf=r";
        let record = parse_dmarc_record(txt).unwrap();

        assert_eq!(record.policy, DmarcPolicy::Reject);
        assert_eq!(record.subdomain_policy, Some(DmarcPolicy::Quarantine));
        assert_eq!(record.percentage, 50);
        assert_eq!(record.dkim_alignment, AlignmentMode::Strict);
        assert_eq!(record.spf_alignment, AlignmentMode::Relaxed);
    }

    #[test]
    fn test_check_alignment_strict() {
        assert!(check_alignment(
            "example.com",
            "example.com",
            AlignmentMode::Strict
        ));
        assert!(!check_alignment(
            "mail.example.com",
            "example.com",
            AlignmentMode::Strict
        ));
    }

    #[test]
    fn test_check_alignment_relaxed() {
        assert!(check_alignment(
            "example.com",
            "example.com",
            AlignmentMode::Relaxed
        ));
        assert!(check_alignment(
            "mail.example.com",
            "example.com",
            AlignmentMode::Relaxed
        ));
        assert!(check_alignment(
            "example.com",
            "mail.example.com",
            AlignmentMode::Relaxed
        ));
    }

    #[test]
    fn test_get_organizational_domain() {
        assert_eq!(
            get_organizational_domain("mail.example.com"),
            Some("example.com".to_string())
        );
        assert_eq!(
            get_organizational_domain("example.com"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_dmarc_result_header_value() {
        assert_eq!(DmarcResult::Pass.as_header_value(), "pass");
        assert_eq!(DmarcResult::Fail(DmarcPolicy::Reject).as_header_value(), "fail");
        assert_eq!(DmarcResult::None.as_header_value(), "none");
    }
}
