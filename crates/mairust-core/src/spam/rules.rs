//! Rule-based spam filtering
//!
//! Provides basic spam detection using simple rules.
//! This serves as a fallback when rspamd is not available.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// A spam detection rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamRule {
    /// Rule name/identifier
    pub name: String,
    /// Rule description
    pub description: String,
    /// Rule type
    #[serde(rename = "type")]
    pub rule_type: RuleType,
    /// Pattern to match (regex or exact string depending on type)
    pub pattern: String,
    /// Score to add when rule matches (positive = spam, negative = ham)
    pub score: f64,
    /// Whether the rule is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Type of spam rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    /// Match pattern in headers
    Header,
    /// Match pattern in body
    Body,
    /// Match sender address/domain
    From,
    /// Match subject line
    Subject,
    /// Check for specific header existence
    HasHeader,
    /// Check for missing header
    MissingHeader,
    /// Check body length
    BodyLength,
    /// URL pattern matching
    Url,
}

/// Result of rule-based spam check
#[derive(Debug, Clone, Default)]
pub struct RuleResult {
    /// Aggregate spam score
    pub score: f64,
    /// Names of matched rules
    pub matched_rules: Vec<String>,
    /// Details of each match
    pub matches: Vec<RuleMatch>,
}

/// A single rule match
#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub rule_name: String,
    pub score: f64,
    pub description: String,
}

/// Rule-based spam filter
pub struct RuleBasedFilter {
    rules: Vec<SpamRule>,
    compiled_patterns: HashMap<String, Regex>,
}

impl RuleBasedFilter {
    /// Create a new rule-based filter with default rules
    pub fn new() -> Self {
        let mut filter = Self {
            rules: Vec::new(),
            compiled_patterns: HashMap::new(),
        };

        // Add default spam rules
        filter.add_default_rules();
        filter
    }

    /// Add default spam detection rules
    fn add_default_rules(&mut self) {
        let default_rules = vec![
            // Subject patterns
            SpamRule {
                name: "SUBJECT_ALL_CAPS".to_string(),
                description: "Subject line is all uppercase".to_string(),
                rule_type: RuleType::Subject,
                pattern: r"^[A-Z\s\d!?.,]+$".to_string(),
                score: 2.0,
                enabled: true,
            },
            SpamRule {
                name: "SUBJECT_URGENCY".to_string(),
                description: "Subject contains urgency words".to_string(),
                rule_type: RuleType::Subject,
                pattern: r"(?i)(urgent|immediate|action required|act now|limited time|expire)".to_string(),
                score: 1.5,
                enabled: true,
            },
            SpamRule {
                name: "SUBJECT_FREE".to_string(),
                description: "Subject contains 'free' spam words".to_string(),
                rule_type: RuleType::Subject,
                pattern: r"(?i)\b(free|gratis|costless)\b".to_string(),
                score: 1.0,
                enabled: true,
            },
            SpamRule {
                name: "SUBJECT_MONEY".to_string(),
                description: "Subject mentions money/prizes".to_string(),
                rule_type: RuleType::Subject,
                pattern: r"(?i)(win|prize|lottery|million|cash|money|bitcoin|crypto)".to_string(),
                score: 2.0,
                enabled: true,
            },
            SpamRule {
                name: "SUBJECT_ADULT".to_string(),
                description: "Subject contains adult content words".to_string(),
                rule_type: RuleType::Subject,
                pattern: r"(?i)(viagra|cialis|porn|xxx|nude|sex)".to_string(),
                score: 5.0,
                enabled: true,
            },

            // Body patterns
            SpamRule {
                name: "BODY_BIG_MONEY".to_string(),
                description: "Body mentions large sums of money".to_string(),
                rule_type: RuleType::Body,
                pattern: r"(?i)\$\s*\d{1,3}(?:,\d{3})*(?:\.\d{2})?\s*(million|billion)?".to_string(),
                score: 2.0,
                enabled: true,
            },
            SpamRule {
                name: "BODY_CLICK_HERE".to_string(),
                description: "Body contains 'click here' links".to_string(),
                rule_type: RuleType::Body,
                pattern: r"(?i)(click here|click below|click this link|click to)".to_string(),
                score: 1.0,
                enabled: true,
            },
            SpamRule {
                name: "BODY_UNSUBSCRIBE_FAKE".to_string(),
                description: "Body has suspicious unsubscribe text".to_string(),
                rule_type: RuleType::Body,
                pattern: r"(?i)(to stop receiving|unsubscribe from this list|opt.?out)".to_string(),
                score: 0.5,
                enabled: true,
            },
            SpamRule {
                name: "BODY_NIGERIAN_PRINCE".to_string(),
                description: "Body matches Nigerian prince scam patterns".to_string(),
                rule_type: RuleType::Body,
                pattern: r"(?i)(prince|inheritance|beneficiary|next of kin|dying wish)".to_string(),
                score: 4.0,
                enabled: true,
            },
            SpamRule {
                name: "BODY_PHISHING".to_string(),
                description: "Body contains phishing patterns".to_string(),
                rule_type: RuleType::Body,
                pattern: r"(?i)(verify your account|confirm your identity|update your (password|details)|suspend.*(account|access))".to_string(),
                score: 3.0,
                enabled: true,
            },

            // URL patterns
            SpamRule {
                name: "URL_SHORTENER".to_string(),
                description: "Contains URL shortener links".to_string(),
                rule_type: RuleType::Url,
                pattern: r"(?i)(bit\.ly|tinyurl\.com|goo\.gl|t\.co|ow\.ly|is\.gd|buff\.ly)".to_string(),
                score: 1.5,
                enabled: true,
            },
            SpamRule {
                name: "URL_IP_ADDRESS".to_string(),
                description: "Contains URL with IP address".to_string(),
                rule_type: RuleType::Url,
                pattern: r"https?://\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}".to_string(),
                score: 3.0,
                enabled: true,
            },

            // Header checks
            SpamRule {
                name: "MISSING_DATE".to_string(),
                description: "Missing Date header".to_string(),
                rule_type: RuleType::MissingHeader,
                pattern: "Date".to_string(),
                score: 1.5,
                enabled: true,
            },
            SpamRule {
                name: "MISSING_MESSAGE_ID".to_string(),
                description: "Missing Message-ID header".to_string(),
                rule_type: RuleType::MissingHeader,
                pattern: "Message-ID".to_string(),
                score: 1.0,
                enabled: true,
            },

            // From address patterns
            SpamRule {
                name: "FROM_FREEMAIL".to_string(),
                description: "From free email provider (not necessarily spam)".to_string(),
                rule_type: RuleType::From,
                pattern: r"(?i)@(gmail\.com|yahoo\.com|hotmail\.com|outlook\.com|aol\.com)$".to_string(),
                score: 0.1,
                enabled: true,
            },
            SpamRule {
                name: "FROM_SUSPICIOUS_TLD".to_string(),
                description: "From suspicious top-level domain".to_string(),
                rule_type: RuleType::From,
                pattern: r"(?i)\.(xyz|top|wang|gq|ml|cf|tk|work|click|link|loan|racing)$".to_string(),
                score: 2.0,
                enabled: true,
            },

            // Body length checks
            SpamRule {
                name: "BODY_EMPTY".to_string(),
                description: "Message body is empty".to_string(),
                rule_type: RuleType::BodyLength,
                pattern: "0".to_string(), // Max length = 0
                score: 2.0,
                enabled: true,
            },
        ];

        for rule in default_rules {
            self.add_rule(rule);
        }
    }

    /// Add a custom rule
    pub fn add_rule(&mut self, rule: SpamRule) {
        // Compile regex pattern if applicable
        if matches!(
            rule.rule_type,
            RuleType::Header
                | RuleType::Body
                | RuleType::From
                | RuleType::Subject
                | RuleType::Url
        ) {
            if let Ok(regex) = Regex::new(&rule.pattern) {
                self.compiled_patterns.insert(rule.name.clone(), regex);
            }
        }
        self.rules.push(rule);
    }

    /// Check a message for spam using rules
    pub fn check(&self, raw_message: &[u8]) -> RuleResult {
        let mut result = RuleResult::default();

        // Parse message as string (lossy)
        let message_str = String::from_utf8_lossy(raw_message);

        // Split into headers and body
        let (headers_str, body_str) = if let Some(pos) = message_str.find("\r\n\r\n") {
            (&message_str[..pos], &message_str[pos + 4..])
        } else if let Some(pos) = message_str.find("\n\n") {
            (&message_str[..pos], &message_str[pos + 2..])
        } else {
            (message_str.as_ref(), "")
        };

        // Parse headers into map
        let headers = parse_headers(headers_str);

        // Get specific header values
        let subject = headers.get("subject").map(|s| s.as_str()).unwrap_or("");
        let from = headers.get("from").map(|s| s.as_str()).unwrap_or("");

        // Apply each rule
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            let matched = match rule.rule_type {
                RuleType::Subject => {
                    if let Some(regex) = self.compiled_patterns.get(&rule.name) {
                        regex.is_match(subject)
                    } else {
                        false
                    }
                }
                RuleType::Body => {
                    if let Some(regex) = self.compiled_patterns.get(&rule.name) {
                        regex.is_match(body_str)
                    } else {
                        false
                    }
                }
                RuleType::From => {
                    if let Some(regex) = self.compiled_patterns.get(&rule.name) {
                        regex.is_match(from)
                    } else {
                        false
                    }
                }
                RuleType::Header => {
                    if let Some(regex) = self.compiled_patterns.get(&rule.name) {
                        regex.is_match(headers_str)
                    } else {
                        false
                    }
                }
                RuleType::Url => {
                    if let Some(regex) = self.compiled_patterns.get(&rule.name) {
                        regex.is_match(&message_str)
                    } else {
                        false
                    }
                }
                RuleType::HasHeader => headers.contains_key(&rule.pattern.to_lowercase()),
                RuleType::MissingHeader => !headers.contains_key(&rule.pattern.to_lowercase()),
                RuleType::BodyLength => {
                    if let Ok(max_len) = rule.pattern.parse::<usize>() {
                        body_str.len() <= max_len
                    } else {
                        false
                    }
                }
            };

            if matched {
                debug!("Rule {} matched, adding score {}", rule.name, rule.score);
                result.score += rule.score;
                result.matched_rules.push(rule.name.clone());
                result.matches.push(RuleMatch {
                    rule_name: rule.name.clone(),
                    score: rule.score,
                    description: rule.description.clone(),
                });
            }
        }

        result
    }

    /// Get all rules
    pub fn get_rules(&self) -> &[SpamRule] {
        &self.rules
    }

    /// Enable/disable a rule by name
    pub fn set_rule_enabled(&mut self, name: &str, enabled: bool) -> bool {
        for rule in &mut self.rules {
            if rule.name == name {
                rule.enabled = enabled;
                return true;
            }
        }
        false
    }
}

impl Default for RuleBasedFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse email headers into a map
fn parse_headers(headers_str: &str) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    let mut current_name = String::new();
    let mut current_value = String::new();

    for line in headers_str.lines() {
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

    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_headers() {
        let headers = "From: sender@example.com\r\nTo: receiver@example.com\r\nSubject: Test\r\n";
        let parsed = parse_headers(headers);

        assert_eq!(parsed.get("from"), Some(&"sender@example.com".to_string()));
        assert_eq!(parsed.get("to"), Some(&"receiver@example.com".to_string()));
        assert_eq!(parsed.get("subject"), Some(&"Test".to_string()));
    }

    #[test]
    fn test_spam_detection() {
        let filter = RuleBasedFilter::new();

        // Clean message
        let clean_message = b"From: user@example.com\r\nTo: me@example.com\r\nSubject: Hello\r\nDate: Mon, 1 Jan 2024 12:00:00 +0000\r\nMessage-ID: <123@example.com>\r\n\r\nThis is a normal message.";
        let result = filter.check(clean_message);
        assert!(result.score < 3.0, "Clean message should have low score: {}", result.score);

        // Spammy message
        let spam_message = b"From: user@suspiciousdomain.xyz\r\nTo: me@example.com\r\nSubject: URGENT: You've Won $1,000,000!!!\r\n\r\nCLICK HERE to claim your prize! This is your inheritance.";
        let result = filter.check(spam_message);
        assert!(result.score > 5.0, "Spam message should have high score: {}", result.score);
        assert!(!result.matched_rules.is_empty());
    }

    #[test]
    fn test_missing_headers() {
        let filter = RuleBasedFilter::new();

        // Message missing Date and Message-ID
        let message = b"From: user@example.com\r\nTo: me@example.com\r\nSubject: Test\r\n\r\nBody";
        let result = filter.check(message);

        assert!(
            result.matched_rules.contains(&"MISSING_DATE".to_string())
                || result.matched_rules.contains(&"MISSING_MESSAGE_ID".to_string()),
            "Should detect missing headers"
        );
    }

    #[test]
    fn test_add_custom_rule() {
        let mut filter = RuleBasedFilter::new();

        filter.add_rule(SpamRule {
            name: "CUSTOM_PATTERN".to_string(),
            description: "Custom test pattern".to_string(),
            rule_type: RuleType::Body,
            pattern: r"custom_test_pattern".to_string(),
            score: 10.0,
            enabled: true,
        });

        let message = b"From: user@example.com\r\nSubject: Test\r\n\r\nThis contains custom_test_pattern here.";
        let result = filter.check(message);

        assert!(result.matched_rules.contains(&"CUSTOM_PATTERN".to_string()));
        assert!(result.score >= 10.0);
    }
}
