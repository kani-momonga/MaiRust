//! Spam filtering module
//!
//! Provides spam detection through:
//! - rspamd integration for advanced spam filtering
//! - Rule-based filtering as a fallback

pub mod rspamd;
pub mod rules;

pub use rspamd::{RspamdClient, RspamdConfig, RspamdResult};
pub use rules::{RuleBasedFilter, RuleResult, SpamRule};

use serde::{Deserialize, Serialize};

/// Overall spam check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamCheckResult {
    /// Spam score (0.0 = ham, higher = more spam-like)
    pub score: f64,
    /// Threshold score for spam classification
    pub threshold: f64,
    /// Whether the message is classified as spam
    pub is_spam: bool,
    /// Whether the message should be rejected
    pub is_reject: bool,
    /// Matched rule names/symbols
    pub symbols: Vec<String>,
    /// Action to take
    pub action: SpamAction,
    /// Additional metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Default for SpamCheckResult {
    fn default() -> Self {
        Self {
            score: 0.0,
            threshold: 5.0,
            is_spam: false,
            is_reject: false,
            symbols: Vec::new(),
            action: SpamAction::Accept,
            metadata: serde_json::Value::Null,
        }
    }
}

/// Action to take based on spam check
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpamAction {
    /// Accept the message
    Accept,
    /// Add spam headers but accept
    AddHeader,
    /// Rewrite subject
    RewriteSubject,
    /// Soft reject (greylist)
    SoftReject,
    /// Reject the message
    Reject,
}

impl Default for SpamAction {
    fn default() -> Self {
        SpamAction::Accept
    }
}

/// Combined spam filter that uses rspamd with rule-based fallback
pub struct SpamFilter {
    rspamd: Option<RspamdClient>,
    rules: RuleBasedFilter,
}

impl SpamFilter {
    /// Create a new spam filter with rspamd configuration
    pub fn new(rspamd_config: Option<RspamdConfig>) -> Self {
        let rspamd = rspamd_config.map(RspamdClient::new);
        Self {
            rspamd,
            rules: RuleBasedFilter::new(),
        }
    }

    /// Create a new spam filter with only rule-based filtering
    pub fn rules_only() -> Self {
        Self {
            rspamd: None,
            rules: RuleBasedFilter::new(),
        }
    }

    /// Check a message for spam
    ///
    /// # Arguments
    /// * `raw_message` - The raw RFC 5322 message bytes
    /// * `from` - Envelope sender
    /// * `rcpt` - Envelope recipients
    /// * `client_ip` - Client IP address
    /// * `helo` - HELO/EHLO hostname
    pub async fn check(
        &self,
        raw_message: &[u8],
        from: Option<&str>,
        rcpt: &[&str],
        client_ip: Option<&str>,
        helo: Option<&str>,
    ) -> SpamCheckResult {
        // Try rspamd first if available
        if let Some(ref rspamd) = self.rspamd {
            match rspamd.check(raw_message, from, rcpt, client_ip, helo).await {
                Ok(result) => {
                    return SpamCheckResult {
                        score: result.score,
                        threshold: result.required_score,
                        is_spam: result.is_spam,
                        is_reject: result.action == "reject",
                        symbols: result.symbols.iter().map(|s| s.name.clone()).collect(),
                        action: match result.action.as_str() {
                            "reject" => SpamAction::Reject,
                            "soft reject" | "greylist" => SpamAction::SoftReject,
                            "rewrite subject" => SpamAction::RewriteSubject,
                            "add header" => SpamAction::AddHeader,
                            _ => SpamAction::Accept,
                        },
                        metadata: serde_json::json!({
                            "source": "rspamd",
                            "message_id": result.message_id,
                        }),
                    };
                }
                Err(e) => {
                    tracing::warn!("rspamd check failed, falling back to rules: {}", e);
                }
            }
        }

        // Fall back to rule-based filtering
        let rule_result = self.rules.check(raw_message);
        SpamCheckResult {
            score: rule_result.score,
            threshold: 5.0,
            is_spam: rule_result.score >= 5.0,
            is_reject: rule_result.score >= 10.0,
            symbols: rule_result.matched_rules,
            action: if rule_result.score >= 10.0 {
                SpamAction::Reject
            } else if rule_result.score >= 5.0 {
                SpamAction::AddHeader
            } else {
                SpamAction::Accept
            },
            metadata: serde_json::json!({
                "source": "rules",
            }),
        }
    }

    /// Check if rspamd is available
    pub fn has_rspamd(&self) -> bool {
        self.rspamd.is_some()
    }

    /// Add a custom rule
    pub fn add_rule(&mut self, rule: SpamRule) {
        self.rules.add_rule(rule);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spam_check_result_default() {
        let result = SpamCheckResult::default();
        assert!(!result.is_spam);
        assert!(!result.is_reject);
        assert_eq!(result.action, SpamAction::Accept);
    }
}
