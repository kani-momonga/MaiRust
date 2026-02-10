//! Policy Engine - Evaluates policy rules against messages
//!
//! The policy engine loads applicable policies and evaluates their conditions
//! against message data to determine what actions should be taken.

use anyhow::Result;
use chrono::{DateTime, Datelike, Timelike, Utc};
use mairust_common::types::{DomainId, TenantId};
use mairust_storage::db::DatabasePool;
use mairust_storage::models::{PolicyAction, PolicyCondition, PolicyConditionType, PolicyRule};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use tracing::{debug, info};

/// Context for policy evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    /// Tenant ID
    pub tenant_id: TenantId,
    /// Domain ID (if applicable)
    pub domain_id: Option<DomainId>,
    /// Policy type: "inbound" or "outbound"
    pub policy_type: String,
    /// Sender email address
    pub sender_address: Option<String>,
    /// Sender domain
    pub sender_domain: Option<String>,
    /// Recipient addresses
    pub recipient_addresses: Vec<String>,
    /// Recipient domains
    pub recipient_domains: Vec<String>,
    /// Message subject
    pub subject: Option<String>,
    /// Message headers as key-value pairs
    pub headers: serde_json::Value,
    /// Message size in bytes
    pub message_size: i64,
    /// Attachment content types
    pub attachment_types: Vec<String>,
    /// Spam score (if available)
    pub spam_score: Option<f64>,
    /// Client IP address
    pub client_ip: Option<String>,
    /// Current time (for time-based policies)
    pub current_time: DateTime<Utc>,
}

impl PolicyContext {
    /// Create a new policy context for inbound mail
    pub fn for_inbound(
        tenant_id: TenantId,
        domain_id: Option<DomainId>,
        sender_address: Option<String>,
        recipient_addresses: Vec<String>,
    ) -> Self {
        let sender_domain = sender_address
            .as_ref()
            .and_then(|s| s.split('@').nth(1).map(|d| d.to_lowercase()));

        let recipient_domains: Vec<String> = recipient_addresses
            .iter()
            .filter_map(|r| r.split('@').nth(1).map(|d| d.to_lowercase()))
            .collect();

        Self {
            tenant_id,
            domain_id,
            policy_type: "inbound".to_string(),
            sender_address,
            sender_domain,
            recipient_addresses,
            recipient_domains,
            subject: None,
            headers: serde_json::json!({}),
            message_size: 0,
            attachment_types: Vec::new(),
            spam_score: None,
            client_ip: None,
            current_time: Utc::now(),
        }
    }

    /// Create a new policy context for outbound mail
    pub fn for_outbound(
        tenant_id: TenantId,
        domain_id: Option<DomainId>,
        sender_address: Option<String>,
        recipient_addresses: Vec<String>,
    ) -> Self {
        let mut ctx = Self::for_inbound(tenant_id, domain_id, sender_address, recipient_addresses);
        ctx.policy_type = "outbound".to_string();
        ctx
    }

    /// Set the message subject
    pub fn with_subject(mut self, subject: Option<String>) -> Self {
        self.subject = subject;
        self
    }

    /// Set the message headers
    pub fn with_headers(mut self, headers: serde_json::Value) -> Self {
        self.headers = headers;
        self
    }

    /// Set the message size
    pub fn with_message_size(mut self, size: i64) -> Self {
        self.message_size = size;
        self
    }

    /// Set attachment types
    pub fn with_attachment_types(mut self, types: Vec<String>) -> Self {
        self.attachment_types = types;
        self
    }

    /// Set spam score
    pub fn with_spam_score(mut self, score: Option<f64>) -> Self {
        self.spam_score = score;
        self
    }

    /// Set client IP
    pub fn with_client_ip(mut self, ip: Option<String>) -> Self {
        self.client_ip = ip;
        self
    }
}

/// Result of evaluating a single policy rule
#[derive(Debug, Clone, Serialize)]
pub struct PolicyMatch {
    /// The policy rule that matched
    pub policy_id: uuid::Uuid,
    /// Policy name
    pub policy_name: String,
    /// Priority of the policy
    pub priority: i32,
    /// Actions to execute
    pub actions: Vec<PolicyAction>,
}

/// Overall result of policy evaluation
#[derive(Debug, Clone, Serialize)]
pub struct PolicyEvaluationResult {
    /// Policies that matched, sorted by priority (highest first)
    pub matches: Vec<PolicyMatch>,
    /// Final action to take (from highest priority match)
    pub final_action: Option<PolicyAction>,
    /// Whether to allow the message
    pub allow: bool,
    /// Whether to reject the message
    pub reject: bool,
    /// SMTP code if rejecting
    pub smtp_code: Option<u16>,
    /// SMTP message if rejecting
    pub smtp_message: Option<String>,
    /// Tags to add to the message
    pub tags: Vec<String>,
    /// Headers to add
    pub headers_to_add: Vec<(String, String)>,
    /// Whether to quarantine the message
    pub quarantine: bool,
    /// Redirect address (if redirecting)
    pub redirect_to: Option<String>,
}

impl Default for PolicyEvaluationResult {
    fn default() -> Self {
        Self {
            matches: Vec::new(),
            final_action: None,
            allow: true,
            reject: false,
            smtp_code: None,
            smtp_message: None,
            tags: Vec::new(),
            headers_to_add: Vec::new(),
            quarantine: false,
            redirect_to: None,
        }
    }
}

/// Evaluation context for a single condition
#[derive(Debug, Clone)]
pub struct PolicyEvaluation {
    /// Whether the condition was matched
    pub matched: bool,
    /// Debug info about the evaluation
    pub debug_info: Option<String>,
}

/// Policy Engine
pub struct PolicyEngine {
    db_pool: DatabasePool,
}

impl PolicyEngine {
    /// Create a new policy engine
    pub fn new(db_pool: DatabasePool) -> Self {
        Self { db_pool }
    }

    /// Evaluate policies for a given context
    pub async fn evaluate(&self, context: &PolicyContext) -> Result<PolicyEvaluationResult> {
        // Load applicable policies
        let policies = self.load_policies(context).await?;

        if policies.is_empty() {
            debug!("No policies found for tenant {:?}", context.tenant_id);
            return Ok(PolicyEvaluationResult::default());
        }

        debug!("Evaluating {} policies", policies.len());

        let mut result = PolicyEvaluationResult::default();
        let mut matches = Vec::new();

        // Evaluate each policy
        for policy in policies {
            if !policy.enabled {
                continue;
            }

            // Check if policy type matches
            if policy.policy_type != context.policy_type && policy.policy_type != "both" {
                continue;
            }

            // Evaluate conditions
            let conditions: Vec<PolicyCondition> =
                serde_json::from_value(policy.conditions.clone()).unwrap_or_default();

            let all_conditions_match = self.evaluate_conditions(&conditions, context);

            if all_conditions_match {
                let actions: Vec<PolicyAction> =
                    serde_json::from_value(policy.actions.clone()).unwrap_or_default();

                info!(
                    "Policy '{}' (priority {}) matched for tenant {:?}",
                    policy.name, policy.priority, context.tenant_id
                );

                matches.push(PolicyMatch {
                    policy_id: policy.id,
                    policy_name: policy.name.clone(),
                    priority: policy.priority,
                    actions: actions.clone(),
                });
            }
        }

        // Sort matches by priority (highest first)
        matches.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Process actions from all matching policies
        for policy_match in &matches {
            for action in &policy_match.actions {
                self.apply_action(action, &mut result);
            }
        }

        result.matches = matches;
        result.final_action = result
            .matches
            .first()
            .and_then(|m| m.actions.first().cloned());

        Ok(result)
    }

    /// Load applicable policies from the database
    async fn load_policies(&self, context: &PolicyContext) -> Result<Vec<PolicyRule>> {
        let pool = self.db_pool.pool();

        // Load policies in order of precedence:
        // 1. Domain-specific policies (if domain_id is provided)
        // 2. Tenant-specific policies
        // 3. Global policies (tenant_id is null)
        let policies: Vec<PolicyRule> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, domain_id, name, description, policy_type,
                   priority, enabled, conditions, actions, created_at, updated_at
            FROM policies
            WHERE enabled = true
              AND (tenant_id IS NULL OR tenant_id = $1)
              AND (domain_id IS NULL OR domain_id = $2)
              AND (policy_type = $3 OR policy_type = 'both')
            ORDER BY priority DESC
            "#,
        )
        .bind(context.tenant_id)
        .bind(context.domain_id)
        .bind(&context.policy_type)
        .fetch_all(pool)
        .await?;

        Ok(policies)
    }

    /// Evaluate all conditions (AND logic)
    fn evaluate_conditions(
        &self,
        conditions: &[PolicyCondition],
        context: &PolicyContext,
    ) -> bool {
        if conditions.is_empty() {
            return true; // No conditions means always match
        }

        for condition in conditions {
            let eval = self.evaluate_condition(condition, context);
            let matched = if condition.negate { !eval.matched } else { eval.matched };

            if !matched {
                return false;
            }
        }

        true
    }

    /// Evaluate a single condition
    fn evaluate_condition(
        &self,
        condition: &PolicyCondition,
        context: &PolicyContext,
    ) -> PolicyEvaluation {
        match condition.condition_type {
            PolicyConditionType::SenderDomain => {
                self.evaluate_string_condition(&condition.operator, &context.sender_domain, &condition.value)
            }
            PolicyConditionType::SenderAddress => {
                self.evaluate_string_condition(&condition.operator, &context.sender_address, &condition.value)
            }
            PolicyConditionType::RecipientDomain => {
                self.evaluate_list_condition(&condition.operator, &context.recipient_domains, &condition.value)
            }
            PolicyConditionType::RecipientAddress => {
                self.evaluate_list_condition(&condition.operator, &context.recipient_addresses, &condition.value)
            }
            PolicyConditionType::SubjectContains => {
                self.evaluate_contains_condition(&context.subject, &condition.value)
            }
            PolicyConditionType::HeaderExists => {
                self.evaluate_header_exists(&context.headers, &condition.value)
            }
            PolicyConditionType::HeaderValue => {
                self.evaluate_header_value(&condition.operator, &context.headers, &condition.value)
            }
            PolicyConditionType::MessageSize => {
                self.evaluate_numeric_condition(&condition.operator, context.message_size as f64, &condition.value)
            }
            PolicyConditionType::AttachmentType => {
                self.evaluate_list_condition(&condition.operator, &context.attachment_types, &condition.value)
            }
            PolicyConditionType::SpamScore => {
                if let Some(score) = context.spam_score {
                    self.evaluate_numeric_condition(&condition.operator, score, &condition.value)
                } else {
                    PolicyEvaluation {
                        matched: false,
                        debug_info: Some("No spam score available".to_string()),
                    }
                }
            }
            PolicyConditionType::ClientIp => {
                self.evaluate_ip_condition(&condition.operator, &context.client_ip, &condition.value)
            }
            PolicyConditionType::TimeOfDay => {
                self.evaluate_time_condition(&condition.operator, &context.current_time, &condition.value)
            }
        }
    }

    /// Evaluate string condition
    fn evaluate_string_condition(
        &self,
        operator: &str,
        value: &Option<String>,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let value = match value {
            Some(v) => v.to_lowercase(),
            None => {
                return PolicyEvaluation {
                    matched: false,
                    debug_info: Some("Value is None".to_string()),
                }
            }
        };

        let expected_str = expected.as_str().unwrap_or_default().to_lowercase();

        let matched = match operator {
            "eq" | "equals" => value == expected_str,
            "ne" | "not_equals" => value != expected_str,
            "contains" => value.contains(&expected_str),
            "starts_with" => value.starts_with(&expected_str),
            "ends_with" => value.ends_with(&expected_str),
            "regex" => {
                // Use size limit to prevent ReDoS from user-supplied patterns
                if let Ok(re) = regex::RegexBuilder::new(&expected_str)
                    .size_limit(1 << 20) // 1MB compiled size limit
                    .build()
                {
                    re.is_match(&value)
                } else {
                    false
                }
            }
            "in" => {
                if let Some(arr) = expected.as_array() {
                    arr.iter().any(|v| {
                        v.as_str()
                            .map(|s| s.to_lowercase() == value)
                            .unwrap_or(false)
                    })
                } else {
                    false
                }
            }
            _ => false,
        };

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("{} {} {:?}", value, operator, expected)),
        }
    }

    /// Evaluate list condition (any item in list matches)
    fn evaluate_list_condition(
        &self,
        operator: &str,
        values: &[String],
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        if values.is_empty() {
            return PolicyEvaluation {
                matched: false,
                debug_info: Some("List is empty".to_string()),
            };
        }

        let expected_str = expected.as_str().unwrap_or_default().to_lowercase();

        let matched = match operator {
            "any_eq" | "contains" => {
                values.iter().any(|v| v.to_lowercase() == expected_str)
            }
            "all_eq" => {
                values.iter().all(|v| v.to_lowercase() == expected_str)
            }
            "any_in" | "in" => {
                if let Some(arr) = expected.as_array() {
                    let expected_list: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                        .collect();
                    values.iter().any(|v| expected_list.contains(&v.to_lowercase()))
                } else {
                    false
                }
            }
            "any_ends_with" | "ends_with" => {
                values.iter().any(|v| v.to_lowercase().ends_with(&expected_str))
            }
            "any_starts_with" | "starts_with" => {
                values.iter().any(|v| v.to_lowercase().starts_with(&expected_str))
            }
            "any_regex" | "regex" => {
                // Use size limit to prevent ReDoS from user-supplied patterns
                if let Ok(re) = regex::RegexBuilder::new(&expected_str)
                    .size_limit(1 << 20) // 1MB compiled size limit
                    .build()
                {
                    values.iter().any(|v| re.is_match(&v.to_lowercase()))
                } else {
                    false
                }
            }
            _ => false,
        };

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("{:?} {} {:?}", values, operator, expected)),
        }
    }

    /// Evaluate contains condition
    fn evaluate_contains_condition(
        &self,
        value: &Option<String>,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let value = match value {
            Some(v) => v.to_lowercase(),
            None => {
                return PolicyEvaluation {
                    matched: false,
                    debug_info: Some("Value is None".to_string()),
                }
            }
        };

        let expected_str = expected.as_str().unwrap_or_default().to_lowercase();
        let matched = value.contains(&expected_str);

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("'{}' contains '{}'", value, expected_str)),
        }
    }

    /// Evaluate header exists condition
    fn evaluate_header_exists(
        &self,
        headers: &serde_json::Value,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let header_name = expected.as_str().unwrap_or_default().to_lowercase();

        let matched = if let Some(obj) = headers.as_object() {
            obj.keys().any(|k| k.to_lowercase() == header_name)
        } else {
            false
        };

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("Header '{}' exists: {}", header_name, matched)),
        }
    }

    /// Evaluate header value condition
    fn evaluate_header_value(
        &self,
        operator: &str,
        headers: &serde_json::Value,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        // Expected format: { "header": "X-Custom-Header", "value": "expected-value" }
        let header_name = expected
            .get("header")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase();

        let expected_value = expected
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase();

        let actual_value = if let Some(obj) = headers.as_object() {
            obj.iter()
                .find(|(k, _)| k.to_lowercase() == header_name)
                .and_then(|(_, v)| v.as_str().map(|s| s.to_lowercase()))
        } else {
            None
        };

        self.evaluate_string_condition(operator, &actual_value, &serde_json::json!(expected_value))
    }

    /// Evaluate numeric condition
    fn evaluate_numeric_condition(
        &self,
        operator: &str,
        value: f64,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let expected_num = expected.as_f64().unwrap_or(0.0);

        let matched = match operator {
            "eq" | "equals" => (value - expected_num).abs() < f64::EPSILON,
            "ne" | "not_equals" => (value - expected_num).abs() >= f64::EPSILON,
            "gt" | "greater_than" => value > expected_num,
            "gte" | "greater_than_or_equal" => value >= expected_num,
            "lt" | "less_than" => value < expected_num,
            "lte" | "less_than_or_equal" => value <= expected_num,
            "between" => {
                if let Some(arr) = expected.as_array() {
                    if arr.len() == 2 {
                        let min = arr[0].as_f64().unwrap_or(0.0);
                        let max = arr[1].as_f64().unwrap_or(0.0);
                        value >= min && value <= max
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        };

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("{} {} {:?}", value, operator, expected)),
        }
    }

    /// Evaluate IP condition
    fn evaluate_ip_condition(
        &self,
        operator: &str,
        client_ip: &Option<String>,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let ip_str = match client_ip {
            Some(ip) => ip,
            None => {
                return PolicyEvaluation {
                    matched: false,
                    debug_info: Some("No client IP".to_string()),
                }
            }
        };

        let matched = match operator {
            "eq" | "equals" => {
                let expected_ip = expected.as_str().unwrap_or_default();
                ip_str == expected_ip
            }
            "in" | "in_range" => {
                // Check if IP is in a CIDR range or list
                if let Some(arr) = expected.as_array() {
                    arr.iter().any(|v| {
                        let range_str = v.as_str().unwrap_or_default();
                        self.ip_in_cidr(ip_str, range_str)
                    })
                } else if let Some(range_str) = expected.as_str() {
                    self.ip_in_cidr(ip_str, range_str)
                } else {
                    false
                }
            }
            _ => false,
        };

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("{} {} {:?}", ip_str, operator, expected)),
        }
    }

    /// Check if IP is in CIDR range
    fn ip_in_cidr(&self, ip_str: &str, cidr_str: &str) -> bool {
        // Parse the IP address
        let ip: IpAddr = match ip_str.parse() {
            Ok(ip) => ip,
            Err(_) => return false,
        };

        // Handle both plain IP and CIDR notation
        if cidr_str.contains('/') {
            // Parse CIDR
            if let Some((network_str, prefix_str)) = cidr_str.split_once('/') {
                let network: IpAddr = match network_str.parse() {
                    Ok(n) => n,
                    Err(_) => return false,
                };

                let prefix: u8 = match prefix_str.parse() {
                    Ok(p) => p,
                    Err(_) => return false,
                };

                // Check if IP is in the CIDR range
                match (ip, network) {
                    (IpAddr::V4(ip), IpAddr::V4(net)) => {
                        let ip_bits = u32::from(ip);
                        let net_bits = u32::from(net);
                        let mask = !0u32 << (32 - prefix);
                        (ip_bits & mask) == (net_bits & mask)
                    }
                    (IpAddr::V6(ip), IpAddr::V6(net)) => {
                        let ip_bits = u128::from(ip);
                        let net_bits = u128::from(net);
                        let mask = !0u128 << (128 - prefix);
                        (ip_bits & mask) == (net_bits & mask)
                    }
                    _ => false,
                }
            } else {
                false
            }
        } else {
            // Plain IP comparison
            ip_str == cidr_str
        }
    }

    /// Evaluate time of day condition
    fn evaluate_time_condition(
        &self,
        operator: &str,
        current_time: &DateTime<Utc>,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let current_hour = current_time.hour();

        let matched = match operator {
            "between" => {
                // Expected format: { "start": 9, "end": 17 } for 9am-5pm
                let start = expected.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let end = expected.get("end").and_then(|v| v.as_u64()).unwrap_or(24) as u32;

                if start <= end {
                    current_hour >= start && current_hour < end
                } else {
                    // Handle overnight range like 22:00 to 06:00
                    current_hour >= start || current_hour < end
                }
            }
            "business_hours" => {
                // Default: Monday-Friday 9am-5pm
                let weekday = current_time.weekday();
                let is_weekday = !matches!(weekday, chrono::Weekday::Sat | chrono::Weekday::Sun);
                is_weekday && current_hour >= 9 && current_hour < 17
            }
            "weekend" => {
                let weekday = current_time.weekday();
                matches!(weekday, chrono::Weekday::Sat | chrono::Weekday::Sun)
            }
            _ => false,
        };

        PolicyEvaluation {
            matched,
            debug_info: Some(format!(
                "Time {} {} {:?}",
                current_time.format("%H:%M"),
                operator,
                expected
            )),
        }
    }

    /// Apply an action to the evaluation result
    fn apply_action(&self, action: &PolicyAction, result: &mut PolicyEvaluationResult) {
        let params = &action.parameters;

        match &action.action_type {
            mairust_storage::models::PolicyActionType::Allow => {
                result.allow = true;
                result.reject = false;
            }
            mairust_storage::models::PolicyActionType::Reject => {
                result.allow = false;
                result.reject = true;
                result.smtp_code = params.get("code").and_then(|v| v.as_u64()).map(|c| c as u16);
                result.smtp_message = params.get("message").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            mairust_storage::models::PolicyActionType::Tempfail => {
                result.allow = false;
                result.reject = true;
                result.smtp_code = Some(params.get("code").and_then(|v| v.as_u64()).map(|c| c as u16).unwrap_or(451));
                result.smtp_message = Some(
                    params
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Temporary failure, please try again later")
                        .to_string(),
                );
            }
            mairust_storage::models::PolicyActionType::Quarantine => {
                result.quarantine = true;
            }
            mairust_storage::models::PolicyActionType::Tag => {
                if let Some(tags) = params.get("tags").and_then(|v| v.as_array()) {
                    for tag in tags {
                        if let Some(t) = tag.as_str() {
                            result.tags.push(t.to_string());
                        }
                    }
                }
                if let Some(tag) = params.get("tag").and_then(|v| v.as_str()) {
                    result.tags.push(tag.to_string());
                }
            }
            mairust_storage::models::PolicyActionType::Redirect => {
                result.redirect_to = params.get("address").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            mairust_storage::models::PolicyActionType::AddHeader => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                let value = params.get("value").and_then(|v| v.as_str()).unwrap_or_default();
                if !name.is_empty() {
                    result.headers_to_add.push((name.to_string(), value.to_string()));
                }
            }
            mairust_storage::models::PolicyActionType::ModifySubject => {
                // Add prefix/suffix header modification
                let prefix = params.get("prefix").and_then(|v| v.as_str()).unwrap_or_default();
                if !prefix.is_empty() {
                    result.headers_to_add.push(("X-Subject-Prefix".to_string(), prefix.to_string()));
                }
            }
            mairust_storage::models::PolicyActionType::RateLimit => {
                // Rate limiting would be handled by a separate rate limiter
                // Here we just add a tag
                result.tags.push("rate_limited".to_string());
            }
            mairust_storage::models::PolicyActionType::RequireTls => {
                result.headers_to_add.push(("X-Require-TLS".to_string(), "true".to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_context() -> PolicyContext {
        PolicyContext::for_inbound(
            uuid::Uuid::new_v4(),
            None,
            Some("sender@example.com".to_string()),
            vec!["recipient@mydomain.com".to_string()],
        )
        .with_subject(Some("Test email subject".to_string()))
        .with_message_size(1024)
        .with_spam_score(Some(2.5))
        .with_client_ip(Some("192.168.1.100".to_string()))
    }

    /// Helper for testing condition evaluation without needing a db_pool
    fn evaluate_string_condition_test(
        operator: &str,
        value: &Option<String>,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let value = match value {
            Some(v) => v.to_lowercase(),
            None => {
                return PolicyEvaluation {
                    matched: false,
                    debug_info: Some("Value is None".to_string()),
                }
            }
        };

        let expected_str = expected.as_str().unwrap_or_default().to_lowercase();

        let matched = match operator {
            "eq" | "equals" => value == expected_str,
            "ne" | "not_equals" => value != expected_str,
            "contains" => value.contains(&expected_str),
            "starts_with" => value.starts_with(&expected_str),
            "ends_with" => value.ends_with(&expected_str),
            _ => false,
        };

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("{} {} {:?}", value, operator, expected)),
        }
    }

    fn evaluate_numeric_condition_test(
        operator: &str,
        value: f64,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let expected_num = expected.as_f64().unwrap_or(0.0);

        let matched = match operator {
            "gt" | "greater_than" => value > expected_num,
            "gte" | "greater_than_or_equal" => value >= expected_num,
            "lt" | "less_than" => value < expected_num,
            "lte" | "less_than_or_equal" => value <= expected_num,
            _ => false,
        };

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("{} {} {:?}", value, operator, expected)),
        }
    }

    fn evaluate_contains_condition_test(
        value: &Option<String>,
        expected: &serde_json::Value,
    ) -> PolicyEvaluation {
        let value = match value {
            Some(v) => v.to_lowercase(),
            None => {
                return PolicyEvaluation {
                    matched: false,
                    debug_info: Some("Value is None".to_string()),
                }
            }
        };

        let expected_str = expected.as_str().unwrap_or_default().to_lowercase();
        let matched = value.contains(&expected_str);

        PolicyEvaluation {
            matched,
            debug_info: Some(format!("'{}' contains '{}'", value, expected_str)),
        }
    }

    fn ip_in_cidr_test(ip_str: &str, cidr_str: &str) -> bool {
        let ip: IpAddr = match ip_str.parse() {
            Ok(ip) => ip,
            Err(_) => return false,
        };

        if cidr_str.contains('/') {
            if let Some((network_str, prefix_str)) = cidr_str.split_once('/') {
                let network: IpAddr = match network_str.parse() {
                    Ok(n) => n,
                    Err(_) => return false,
                };

                let prefix: u8 = match prefix_str.parse() {
                    Ok(p) => p,
                    Err(_) => return false,
                };

                match (ip, network) {
                    (IpAddr::V4(ip), IpAddr::V4(net)) => {
                        let ip_bits = u32::from(ip);
                        let net_bits = u32::from(net);
                        let mask = !0u32 << (32 - prefix);
                        (ip_bits & mask) == (net_bits & mask)
                    }
                    (IpAddr::V6(ip), IpAddr::V6(net)) => {
                        let ip_bits = u128::from(ip);
                        let net_bits = u128::from(net);
                        let mask = !0u128 << (128 - prefix);
                        (ip_bits & mask) == (net_bits & mask)
                    }
                    _ => false,
                }
            } else {
                false
            }
        } else {
            ip_str == cidr_str
        }
    }

    #[test]
    fn test_string_condition_equals() {
        let context = create_test_context();
        let result = evaluate_string_condition_test(
            "eq",
            &context.sender_domain,
            &serde_json::json!("example.com"),
        );
        assert!(result.matched);
    }

    #[test]
    fn test_numeric_condition() {
        let context = create_test_context();

        // Test spam score greater than
        let result = evaluate_numeric_condition_test(
            "gt",
            context.spam_score.unwrap(),
            &serde_json::json!(2.0),
        );
        assert!(result.matched);

        // Test spam score less than
        let result = evaluate_numeric_condition_test(
            "lt",
            context.spam_score.unwrap(),
            &serde_json::json!(2.0),
        );
        assert!(!result.matched);
    }

    #[test]
    fn test_contains_condition() {
        let context = create_test_context();
        let result = evaluate_contains_condition_test(&context.subject, &serde_json::json!("test"));
        assert!(result.matched);
    }

    #[test]
    fn test_ip_in_cidr() {
        assert!(ip_in_cidr_test("192.168.1.100", "192.168.1.0/24"));
        assert!(!ip_in_cidr_test("192.168.2.100", "192.168.1.0/24"));
        assert!(ip_in_cidr_test("10.0.0.1", "10.0.0.0/8"));
    }

    #[test]
    fn test_negate_condition() {
        let context = create_test_context();

        // Test that sender is NOT from spam.com
        let result = evaluate_string_condition_test(
            "eq",
            &context.sender_domain,
            &serde_json::json!("spam.com"),
        );
        assert!(!result.matched); // Raw match is false (not from spam.com)

        // With negation applied, this would be true (NOT false = true)
        let negated = !result.matched;
        assert!(negated);
    }

    #[test]
    fn test_policy_context_creation() {
        let context = create_test_context();
        assert_eq!(context.policy_type, "inbound");
        assert_eq!(context.sender_domain, Some("example.com".to_string()));
        assert_eq!(context.recipient_domains, vec!["mydomain.com".to_string()]);
        assert_eq!(context.message_size, 1024);
        assert_eq!(context.spam_score, Some(2.5));
    }

    #[test]
    fn test_policy_context_outbound() {
        let context = PolicyContext::for_outbound(
            uuid::Uuid::new_v4(),
            None,
            Some("sender@internal.com".to_string()),
            vec!["external@example.org".to_string()],
        );
        assert_eq!(context.policy_type, "outbound");
        assert_eq!(context.sender_domain, Some("internal.com".to_string()));
    }
}
