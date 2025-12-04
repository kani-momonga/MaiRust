//! Email Authentication Module
//!
//! Provides SPF, DKIM, and DMARC verification for incoming mail
//! and DKIM signing for outgoing mail.

pub mod dkim;
pub mod dmarc;
pub mod spf;

pub use dkim::{DkimResult, DkimSigner, DkimVerifier};
pub use dmarc::{DmarcPolicy, DmarcResult, DmarcVerifier};
pub use spf::{SpfResult, SpfVerifier};

/// Combined email authentication result
#[derive(Debug, Clone)]
pub struct AuthenticationResult {
    pub spf: SpfResult,
    pub dkim: DkimResult,
    pub dmarc: DmarcResult,
}

impl AuthenticationResult {
    /// Create a new authentication result
    pub fn new(spf: SpfResult, dkim: DkimResult, dmarc: DmarcResult) -> Self {
        Self { spf, dkim, dmarc }
    }

    /// Check if the message should be accepted based on authentication results
    pub fn should_accept(&self) -> bool {
        // Accept if SPF passes or softfails, and DMARC doesn't reject
        let spf_ok = matches!(
            self.spf,
            SpfResult::Pass | SpfResult::SoftFail | SpfResult::Neutral | SpfResult::None
        );

        let dmarc_ok = !matches!(self.dmarc, DmarcResult::Fail(DmarcPolicy::Reject));

        spf_ok && dmarc_ok
    }

    /// Generate Authentication-Results header value
    pub fn to_header(&self, hostname: &str) -> String {
        format!(
            "{}; spf={} dkim={} dmarc={}",
            hostname,
            self.spf.as_header_value(),
            self.dkim.as_header_value(),
            self.dmarc.as_header_value()
        )
    }
}
