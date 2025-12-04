//! rspamd integration for spam filtering
//!
//! Connects to rspamd via its HTTP API to check messages for spam.
//! See: https://rspamd.com/doc/architecture/protocol.html

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, warn};

/// rspamd client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RspamdConfig {
    /// rspamd HTTP endpoint (default: http://localhost:11333)
    #[serde(default = "default_url")]
    pub url: String,
    /// Request timeout in milliseconds
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Password for rspamd (if configured)
    pub password: Option<String>,
    /// Whether to reject on rspamd errors
    #[serde(default)]
    pub reject_on_error: bool,
}

fn default_url() -> String {
    "http://localhost:11333".to_string()
}

fn default_timeout() -> u64 {
    5000
}

impl Default for RspamdConfig {
    fn default() -> Self {
        Self {
            url: default_url(),
            timeout_ms: default_timeout(),
            password: None,
            reject_on_error: false,
        }
    }
}

/// rspamd check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RspamdResult {
    /// Spam score
    pub score: f64,
    /// Required score for spam classification
    pub required_score: f64,
    /// Whether the message is classified as spam
    pub is_spam: bool,
    /// Recommended action
    pub action: String,
    /// Matched symbols/rules
    pub symbols: Vec<RspamdSymbol>,
    /// Message-ID if present
    pub message_id: Option<String>,
    /// Time taken to process (ms)
    pub time_real: Option<f64>,
}

/// rspamd symbol/rule match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RspamdSymbol {
    /// Symbol name
    pub name: String,
    /// Symbol score
    pub score: f64,
    /// Symbol description
    pub description: Option<String>,
    /// Symbol options
    #[serde(default)]
    pub options: Vec<String>,
}

/// Raw rspamd API response
#[derive(Debug, Deserialize)]
struct RspamdApiResponse {
    #[serde(default)]
    score: f64,
    #[serde(default = "default_required_score")]
    required_score: f64,
    #[serde(default)]
    action: String,
    #[serde(default)]
    is_spam: Option<bool>,
    #[serde(default)]
    is_skipped: bool,
    #[serde(default)]
    symbols: HashMap<String, RspamdApiSymbol>,
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    time_real: Option<f64>,
}

fn default_required_score() -> f64 {
    5.0
}

#[derive(Debug, Deserialize)]
struct RspamdApiSymbol {
    #[serde(default)]
    score: f64,
    description: Option<String>,
    #[serde(default)]
    options: Vec<String>,
}

/// rspamd HTTP client
pub struct RspamdClient {
    config: RspamdConfig,
    client: Client,
}

impl RspamdClient {
    /// Create a new rspamd client
    pub fn new(config: RspamdConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    /// Check a message for spam
    ///
    /// # Arguments
    /// * `raw_message` - The raw RFC 5322 message bytes
    /// * `from` - Envelope sender (MAIL FROM)
    /// * `rcpt` - Envelope recipients (RCPT TO)
    /// * `client_ip` - Client IP address
    /// * `helo` - HELO/EHLO hostname
    pub async fn check(
        &self,
        raw_message: &[u8],
        from: Option<&str>,
        rcpt: &[&str],
        client_ip: Option<&str>,
        helo: Option<&str>,
    ) -> Result<RspamdResult> {
        let url = format!("{}/checkv2", self.config.url);

        debug!("Checking message with rspamd at {}", url);

        // Build request with headers
        let mut request = self.client.post(&url).body(raw_message.to_vec());

        // Add rspamd headers
        if let Some(from) = from {
            request = request.header("From", from);
        }

        for rcpt_addr in rcpt {
            request = request.header("Rcpt", *rcpt_addr);
        }

        if let Some(ip) = client_ip {
            request = request.header("IP", ip);
        }

        if let Some(helo) = helo {
            request = request.header("Helo", helo);
        }

        // Add password if configured
        if let Some(ref password) = self.config.password {
            request = request.header("Password", password);
        }

        // Send request
        let response = request.send().await.map_err(|e| {
            warn!("rspamd request failed: {}", e);
            anyhow!("rspamd request failed: {}", e)
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "rspamd returned error status {}: {}",
                status,
                body
            ));
        }

        // Parse response
        let api_response: RspamdApiResponse = response.json().await.map_err(|e| {
            warn!("Failed to parse rspamd response: {}", e);
            anyhow!("Failed to parse rspamd response: {}", e)
        })?;

        // Convert symbols
        let symbols: Vec<RspamdSymbol> = api_response
            .symbols
            .into_iter()
            .map(|(name, sym)| RspamdSymbol {
                name,
                score: sym.score,
                description: sym.description,
                options: sym.options,
            })
            .collect();

        // Determine if spam
        let is_spam = api_response.is_spam.unwrap_or_else(|| {
            api_response.score >= api_response.required_score
                || matches!(
                    api_response.action.as_str(),
                    "reject" | "rewrite subject" | "add header" | "soft reject"
                )
        });

        Ok(RspamdResult {
            score: api_response.score,
            required_score: api_response.required_score,
            is_spam,
            action: api_response.action,
            symbols,
            message_id: api_response.message_id,
            time_real: api_response.time_real,
        })
    }

    /// Learn a message as spam or ham
    ///
    /// # Arguments
    /// * `raw_message` - The raw RFC 5322 message bytes
    /// * `is_spam` - Whether the message is spam (true) or ham (false)
    pub async fn learn(&self, raw_message: &[u8], is_spam: bool) -> Result<()> {
        let endpoint = if is_spam { "learnspam" } else { "learnham" };
        let url = format!("{}/{}", self.config.url, endpoint);

        debug!("Learning message as {} via {}", endpoint, url);

        let mut request = self.client.post(&url).body(raw_message.to_vec());

        // Add password if configured
        if let Some(ref password) = self.config.password {
            request = request.header("Password", password);
        }

        let response = request.send().await.map_err(|e| {
            warn!("rspamd learn request failed: {}", e);
            anyhow!("rspamd learn request failed: {}", e)
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "rspamd learn returned error status {}: {}",
                status,
                body
            ));
        }

        Ok(())
    }

    /// Fuzzy add a message (for fuzzy hash blocking)
    pub async fn fuzzy_add(&self, raw_message: &[u8], flag: u32, weight: u32) -> Result<()> {
        let url = format!("{}/fuzzyadd", self.config.url);

        debug!("Adding fuzzy hash via {}", url);

        let mut request = self
            .client
            .post(&url)
            .body(raw_message.to_vec())
            .header("Flag", flag.to_string())
            .header("Weight", weight.to_string());

        // Add password if configured
        if let Some(ref password) = self.config.password {
            request = request.header("Password", password);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "rspamd fuzzy add returned error status {}: {}",
                status,
                body
            ));
        }

        Ok(())
    }

    /// Get rspamd statistics
    pub async fn get_stats(&self) -> Result<RspamdStats> {
        let url = format!("{}/stat", self.config.url);

        let mut request = self.client.get(&url);

        if let Some(ref password) = self.config.password {
            request = request.header("Password", password);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(anyhow!("rspamd stat returned error status {}", status));
        }

        let stats: RspamdStats = response.json().await?;
        Ok(stats)
    }

    /// Check if rspamd is healthy
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/ping", self.config.url);
        match self.client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}

/// rspamd statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RspamdStats {
    #[serde(default)]
    pub scanned: u64,
    #[serde(default)]
    pub learned: u64,
    #[serde(default)]
    pub spam_count: u64,
    #[serde(default)]
    pub ham_count: u64,
    #[serde(default)]
    pub connections: u64,
    #[serde(default)]
    pub control_connections: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = RspamdConfig::default();
        assert_eq!(config.url, "http://localhost:11333");
        assert_eq!(config.timeout_ms, 5000);
        assert!(!config.reject_on_error);
    }

    #[test]
    fn test_rspamd_result_serialization() {
        let result = RspamdResult {
            score: 5.5,
            required_score: 5.0,
            is_spam: true,
            action: "add header".to_string(),
            symbols: vec![RspamdSymbol {
                name: "BAYES_SPAM".to_string(),
                score: 3.0,
                description: Some("Bayesian spam probability".to_string()),
                options: vec![],
            }],
            message_id: Some("<test@example.com>".to_string()),
            time_real: Some(10.5),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("BAYES_SPAM"));
    }
}
