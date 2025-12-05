//! AI Categorization Plugin
//!
//! Provides email categorization using AI and rule-based systems.

use super::types::{Plugin, PluginContext, PluginError, PluginHealth, PluginInfo, PluginProtocol, PluginResult, PluginStatus, PluginCapability};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Input for categorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorizationInput {
    /// Message ID
    pub message_id: Uuid,
    /// From address
    pub from_address: Option<String>,
    /// To addresses
    pub to_addresses: Vec<String>,
    /// Subject
    pub subject: Option<String>,
    /// Body preview (first N characters)
    pub body_preview: Option<String>,
    /// Headers as key-value pairs
    pub headers: HashMap<String, String>,
    /// Spam score (if available)
    pub spam_score: Option<f64>,
    /// Existing tags
    pub tags: Vec<String>,
}

/// Output from categorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorizationOutput {
    /// Assigned category ID
    pub category_id: Uuid,
    /// Category name
    pub category_name: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// AI-generated summary (optional)
    pub summary: Option<String>,
    /// Suggested tags
    pub suggested_tags: Vec<String>,
    /// Additional metadata
    pub metadata: serde_json::Value,
}

/// AI Categorization Plugin trait
#[async_trait]
pub trait AiCategorizationPlugin: Plugin {
    /// Categorize a message
    async fn categorize(
        &self,
        ctx: &PluginContext,
        input: &CategorizationInput,
    ) -> PluginResult<CategorizationOutput>;

    /// Batch categorize messages
    async fn categorize_batch(
        &self,
        ctx: &PluginContext,
        inputs: &[CategorizationInput],
    ) -> PluginResult<Vec<CategorizationOutput>> {
        let mut results = Vec::with_capacity(inputs.len());
        for input in inputs {
            results.push(self.categorize(ctx, input).await?);
        }
        Ok(results)
    }

    /// Train or update the model with feedback
    async fn feedback(
        &self,
        _ctx: &PluginContext,
        _message_id: Uuid,
        _correct_category_id: Uuid,
    ) -> PluginResult<()> {
        // Default implementation does nothing
        Ok(())
    }
}

/// Default categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultCategory {
    Primary,
    Social,
    Promotions,
    Updates,
    Forums,
    Spam,
}

impl DefaultCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Social => "Social",
            Self::Promotions => "Promotions",
            Self::Updates => "Updates",
            Self::Forums => "Forums",
            Self::Spam => "Spam",
        }
    }
}

/// Rule-based categorizer (built-in, no AI required)
pub struct RuleBasedCategorizer {
    info: PluginInfo,
    status: PluginStatus,
    /// Category ID mappings
    category_ids: HashMap<String, Uuid>,
    /// Social domain patterns
    social_domains: Vec<String>,
    /// Promotional patterns
    promo_patterns: Vec<String>,
    /// Update patterns (automated notifications)
    update_patterns: Vec<String>,
    /// Forum/mailing list patterns
    forum_patterns: Vec<String>,
}

impl RuleBasedCategorizer {
    /// Create a new rule-based categorizer
    pub fn new() -> Self {
        Self {
            info: PluginInfo {
                id: "mairust.builtin.rule-categorizer".to_string(),
                name: "Rule-Based Categorizer".to_string(),
                version: "1.0.0".to_string(),
                description: Some("Built-in rule-based email categorization".to_string()),
                author: Some("MaiRust".to_string()),
                homepage: None,
                capabilities: vec![
                    PluginCapability::ReadHeaders,
                    PluginCapability::ReadBodyPreview,
                    PluginCapability::WriteCategory,
                    PluginCapability::WriteTags,
                ],
                protocol: PluginProtocol::Native,
            },
            status: PluginStatus::Active,
            category_ids: HashMap::new(),
            social_domains: vec![
                "facebook.com".to_string(),
                "twitter.com".to_string(),
                "linkedin.com".to_string(),
                "instagram.com".to_string(),
                "tiktok.com".to_string(),
                "snapchat.com".to_string(),
                "pinterest.com".to_string(),
            ],
            promo_patterns: vec![
                "unsubscribe".to_string(),
                "sale".to_string(),
                "discount".to_string(),
                "offer".to_string(),
                "deal".to_string(),
                "promo".to_string(),
                "limited time".to_string(),
                "% off".to_string(),
            ],
            update_patterns: vec![
                "notification".to_string(),
                "alert".to_string(),
                "your order".to_string(),
                "shipping".to_string(),
                "delivery".to_string(),
                "receipt".to_string(),
                "invoice".to_string(),
                "statement".to_string(),
                "password reset".to_string(),
                "verification".to_string(),
            ],
            forum_patterns: vec![
                "mailing list".to_string(),
                "list-unsubscribe".to_string(),
                "[".to_string(), // Common in mailing list subjects
                "digest".to_string(),
                "forum".to_string(),
            ],
        }
    }

    /// Set category ID mappings
    pub fn with_category_ids(mut self, ids: HashMap<String, Uuid>) -> Self {
        self.category_ids = ids;
        self
    }

    /// Check if a domain is a social network
    fn is_social_domain(&self, from: &str) -> bool {
        let from_lower = from.to_lowercase();
        self.social_domains.iter().any(|d| from_lower.contains(d))
    }

    /// Check if content matches promotional patterns
    fn is_promotional(&self, subject: &str, body: &str) -> bool {
        let combined = format!("{} {}", subject, body).to_lowercase();
        self.promo_patterns.iter().filter(|p| combined.contains(*p)).count() >= 2
    }

    /// Check if content matches update patterns
    fn is_update(&self, subject: &str, body: &str, headers: &HashMap<String, String>) -> bool {
        // Check for automated emails
        if headers.get("X-Auto-Response-Suppress").is_some() ||
           headers.get("Auto-Submitted").is_some() ||
           headers.get("Precedence").map(|v| v == "bulk").unwrap_or(false) {
            return true;
        }

        let combined = format!("{} {}", subject, body).to_lowercase();
        self.update_patterns.iter().any(|p| combined.contains(p))
    }

    /// Check if content is from a mailing list/forum
    fn is_forum(&self, subject: &str, headers: &HashMap<String, String>) -> bool {
        // Check for List-* headers
        if headers.keys().any(|k| k.to_lowercase().starts_with("list-")) {
            return true;
        }

        let subject_lower = subject.to_lowercase();
        self.forum_patterns.iter().any(|p| subject_lower.contains(p))
    }

    fn get_category_id(&self, name: &str) -> Uuid {
        self.category_ids.get(name).copied().unwrap_or_else(Uuid::new_v4)
    }
}

impl Default for RuleBasedCategorizer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for RuleBasedCategorizer {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    async fn initialize(&mut self) -> PluginResult<()> {
        self.status = PluginStatus::Active;
        Ok(())
    }

    async fn shutdown(&mut self) -> PluginResult<()> {
        self.status = PluginStatus::Stopped;
        Ok(())
    }

    async fn health_check(&self) -> PluginResult<PluginHealth> {
        Ok(PluginHealth {
            status: self.status,
            last_check: Utc::now(),
            message: None,
            error_count: 0,
            success_count: 0,
            avg_response_ms: 0.1, // Very fast (native)
        })
    }

    fn status(&self) -> PluginStatus {
        self.status
    }
}

#[async_trait]
impl AiCategorizationPlugin for RuleBasedCategorizer {
    async fn categorize(
        &self,
        _ctx: &PluginContext,
        input: &CategorizationInput,
    ) -> PluginResult<CategorizationOutput> {
        let from = input.from_address.as_deref().unwrap_or("");
        let subject = input.subject.as_deref().unwrap_or("");
        let body = input.body_preview.as_deref().unwrap_or("");

        // Check spam first
        if let Some(score) = input.spam_score {
            if score > 5.0 {
                return Ok(CategorizationOutput {
                    category_id: self.get_category_id("Spam"),
                    category_name: "Spam".to_string(),
                    confidence: (score / 10.0).min(1.0) as f32,
                    summary: None,
                    suggested_tags: vec!["spam".to_string()],
                    metadata: serde_json::json!({"spam_score": score}),
                });
            }
        }

        // Check social
        if self.is_social_domain(from) {
            return Ok(CategorizationOutput {
                category_id: self.get_category_id("Social"),
                category_name: "Social".to_string(),
                confidence: 0.85,
                summary: None,
                suggested_tags: vec!["social".to_string()],
                metadata: serde_json::json!({}),
            });
        }

        // Check forum/mailing list
        if self.is_forum(subject, &input.headers) {
            return Ok(CategorizationOutput {
                category_id: self.get_category_id("Forums"),
                category_name: "Forums".to_string(),
                confidence: 0.80,
                summary: None,
                suggested_tags: vec!["forum".to_string(), "mailing-list".to_string()],
                metadata: serde_json::json!({}),
            });
        }

        // Check promotional
        if self.is_promotional(subject, body) {
            return Ok(CategorizationOutput {
                category_id: self.get_category_id("Promotions"),
                category_name: "Promotions".to_string(),
                confidence: 0.75,
                summary: None,
                suggested_tags: vec!["promotion".to_string(), "marketing".to_string()],
                metadata: serde_json::json!({}),
            });
        }

        // Check updates
        if self.is_update(subject, body, &input.headers) {
            return Ok(CategorizationOutput {
                category_id: self.get_category_id("Updates"),
                category_name: "Updates".to_string(),
                confidence: 0.70,
                summary: None,
                suggested_tags: vec!["notification".to_string()],
                metadata: serde_json::json!({}),
            });
        }

        // Default to Primary
        Ok(CategorizationOutput {
            category_id: self.get_category_id("Primary"),
            category_name: "Primary".to_string(),
            confidence: 0.60,
            summary: None,
            suggested_tags: vec![],
            metadata: serde_json::json!({}),
        })
    }
}

/// Default AI categorizer (placeholder for external AI service)
pub struct DefaultAiCategorizer {
    info: PluginInfo,
    status: PluginStatus,
    /// Fallback to rule-based when AI is unavailable
    fallback: RuleBasedCategorizer,
    /// AI service endpoint
    endpoint: Option<String>,
}

impl DefaultAiCategorizer {
    /// Create a new AI categorizer
    pub fn new() -> Self {
        Self {
            info: PluginInfo {
                id: "mairust.builtin.ai-categorizer".to_string(),
                name: "AI Categorizer".to_string(),
                version: "1.0.0".to_string(),
                description: Some("AI-powered email categorization with rule-based fallback".to_string()),
                author: Some("MaiRust".to_string()),
                homepage: None,
                capabilities: vec![
                    PluginCapability::ReadHeaders,
                    PluginCapability::ReadBodyPreview,
                    PluginCapability::WriteCategory,
                    PluginCapability::WriteTags,
                ],
                protocol: PluginProtocol::Native,
            },
            status: PluginStatus::Active,
            fallback: RuleBasedCategorizer::new(),
            endpoint: None,
        }
    }

    /// Set the AI service endpoint
    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.info.protocol = PluginProtocol::Http { endpoint: endpoint.clone() };
        self.endpoint = Some(endpoint);
        self
    }

    /// Set category ID mappings
    pub fn with_category_ids(mut self, ids: HashMap<String, Uuid>) -> Self {
        self.fallback = self.fallback.with_category_ids(ids);
        self
    }
}

impl Default for DefaultAiCategorizer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for DefaultAiCategorizer {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    async fn initialize(&mut self) -> PluginResult<()> {
        // Initialize fallback
        self.fallback.initialize().await?;

        // If endpoint is configured, try to connect
        if let Some(_endpoint) = &self.endpoint {
            // TODO: Implement actual AI service health check
            // For now, just mark as active
        }

        self.status = PluginStatus::Active;
        Ok(())
    }

    async fn shutdown(&mut self) -> PluginResult<()> {
        self.fallback.shutdown().await?;
        self.status = PluginStatus::Stopped;
        Ok(())
    }

    async fn health_check(&self) -> PluginResult<PluginHealth> {
        Ok(PluginHealth {
            status: self.status,
            last_check: Utc::now(),
            message: Some(if self.endpoint.is_some() {
                "AI service configured".to_string()
            } else {
                "Using rule-based fallback".to_string()
            }),
            error_count: 0,
            success_count: 0,
            avg_response_ms: if self.endpoint.is_some() { 50.0 } else { 0.1 },
        })
    }

    fn status(&self) -> PluginStatus {
        self.status
    }
}

#[async_trait]
impl AiCategorizationPlugin for DefaultAiCategorizer {
    async fn categorize(
        &self,
        ctx: &PluginContext,
        input: &CategorizationInput,
    ) -> PluginResult<CategorizationOutput> {
        // If no endpoint, use fallback
        if self.endpoint.is_none() {
            return self.fallback.categorize(ctx, input).await;
        }

        // TODO: Implement actual AI service call
        // For now, use fallback
        self.fallback.categorize(ctx, input).await
    }

    async fn feedback(
        &self,
        _ctx: &PluginContext,
        _message_id: Uuid,
        _correct_category_id: Uuid,
    ) -> PluginResult<()> {
        // TODO: Implement feedback mechanism for AI training
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rule_based_social() {
        let categorizer = RuleBasedCategorizer::new();
        let ctx = PluginContext::new(Uuid::new_v4());

        let input = CategorizationInput {
            message_id: Uuid::new_v4(),
            from_address: Some("notifications@facebook.com".to_string()),
            to_addresses: vec!["user@example.com".to_string()],
            subject: Some("You have new friend requests".to_string()),
            body_preview: None,
            headers: HashMap::new(),
            spam_score: None,
            tags: vec![],
        };

        let result = categorizer.categorize(&ctx, &input).await.unwrap();
        assert_eq!(result.category_name, "Social");
    }

    #[tokio::test]
    async fn test_rule_based_promo() {
        let categorizer = RuleBasedCategorizer::new();
        let ctx = PluginContext::new(Uuid::new_v4());

        let input = CategorizationInput {
            message_id: Uuid::new_v4(),
            from_address: Some("sales@store.com".to_string()),
            to_addresses: vec!["user@example.com".to_string()],
            subject: Some("50% OFF Sale - Limited Time Offer!".to_string()),
            body_preview: Some("Don't miss this amazing discount. Unsubscribe link below.".to_string()),
            headers: HashMap::new(),
            spam_score: None,
            tags: vec![],
        };

        let result = categorizer.categorize(&ctx, &input).await.unwrap();
        assert_eq!(result.category_name, "Promotions");
    }

    #[tokio::test]
    async fn test_rule_based_primary() {
        let categorizer = RuleBasedCategorizer::new();
        let ctx = PluginContext::new(Uuid::new_v4());

        let input = CategorizationInput {
            message_id: Uuid::new_v4(),
            from_address: Some("colleague@company.com".to_string()),
            to_addresses: vec!["user@example.com".to_string()],
            subject: Some("Meeting tomorrow".to_string()),
            body_preview: Some("Hi, can we meet at 10am tomorrow?".to_string()),
            headers: HashMap::new(),
            spam_score: None,
            tags: vec![],
        };

        let result = categorizer.categorize(&ctx, &input).await.unwrap();
        assert_eq!(result.category_name, "Primary");
    }
}
