//! Template Renderer - Handles personalization of email content

use mairust_storage::models::Recipient;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

/// Template renderer for personalizing email content
pub struct TemplateRenderer {
    /// Base URL for unsubscribe links
    unsubscribe_base_url: String,
}

impl TemplateRenderer {
    /// Create a new template renderer
    pub fn new(unsubscribe_base_url: String) -> Self {
        Self { unsubscribe_base_url }
    }

    /// Render a template with recipient data
    pub fn render(&self, template: &str, recipient: &Recipient, campaign_id: Option<uuid::Uuid>) -> String {
        let mut result = template.to_string();

        // Basic variables
        result = result.replace("{{email}}", &recipient.email);
        result = result.replace("{{name}}", recipient.name.as_deref().unwrap_or(""));

        // Split name into first/last (simple heuristic)
        if let Some(name) = &recipient.name {
            let parts: Vec<&str> = name.split_whitespace().collect();
            let first_name = parts.first().unwrap_or(&"");
            let last_name = if parts.len() > 1 {
                parts[1..].join(" ")
            } else {
                String::new()
            };
            result = result.replace("{{first_name}}", first_name);
            result = result.replace("{{last_name}}", &last_name);
        } else {
            result = result.replace("{{first_name}}", "");
            result = result.replace("{{last_name}}", "");
        }

        // Custom attributes
        if let Some(attrs) = recipient.attributes.as_object() {
            for (key, value) in attrs {
                let placeholder = format!("{{{{attributes.{}}}}}", key);
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => value.to_string(),
                };
                result = result.replace(&placeholder, &value_str);
            }
        }

        // Generate unsubscribe URL
        let unsubscribe_token = self.generate_unsubscribe_token(&recipient.email, campaign_id);
        let unsubscribe_url = format!("{}/{}", self.unsubscribe_base_url, unsubscribe_token);
        result = result.replace("{{unsubscribe_url}}", &unsubscribe_url);

        // Clean up any remaining placeholders
        result = self.remove_unused_placeholders(&result);

        result
    }

    /// Render subject line with recipient data
    pub fn render_subject(&self, subject: &str, recipient: &Recipient) -> String {
        let mut result = subject.to_string();

        result = result.replace("{{email}}", &recipient.email);
        result = result.replace("{{name}}", recipient.name.as_deref().unwrap_or(""));

        if let Some(name) = &recipient.name {
            let parts: Vec<&str> = name.split_whitespace().collect();
            let first_name = parts.first().unwrap_or(&"");
            result = result.replace("{{first_name}}", first_name);
        } else {
            result = result.replace("{{first_name}}", "");
        }

        // Custom attributes in subject
        if let Some(attrs) = recipient.attributes.as_object() {
            for (key, value) in attrs {
                let placeholder = format!("{{{{attributes.{}}}}}", key);
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => value.to_string(),
                };
                result = result.replace(&placeholder, &value_str);
            }
        }

        self.remove_unused_placeholders(&result)
    }

    /// Generate unsubscribe token for a recipient
    fn generate_unsubscribe_token(&self, email: &str, campaign_id: Option<uuid::Uuid>) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use sha2::{Digest, Sha256};

        // Create a simple signed token
        // In production, use HMAC with a secret key
        let payload = match campaign_id {
            Some(id) => format!("{}:{}", email, id),
            None => email.to_string(),
        };

        // Simple encoding (in production, use proper signing)
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let hash = hasher.finalize();
        let hash_prefix = &hash[..8];

        let token_data = format!("{}:{}", payload, hex::encode(hash_prefix));
        URL_SAFE_NO_PAD.encode(token_data.as_bytes())
    }

    /// Remove unused placeholder variables
    fn remove_unused_placeholders(&self, content: &str) -> String {
        // Match patterns like {{variable}} or {{attributes.something}}
        let re = Regex::new(r"\{\{[^}]+\}\}").unwrap();
        re.replace_all(content, "").to_string()
    }

    /// Parse unsubscribe token and extract email
    pub fn parse_unsubscribe_token(&self, token: &str) -> Option<(String, Option<uuid::Uuid>)> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let decoded = URL_SAFE_NO_PAD.decode(token).ok()?;
        let token_data = String::from_utf8(decoded).ok()?;

        // Split by last colon to get hash
        let parts: Vec<&str> = token_data.rsplitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }

        let payload = parts[1];
        let _hash_hex = parts[0];

        // TODO: Verify hash in production

        // Parse payload
        if let Some((email, campaign_id_str)) = payload.split_once(':') {
            let campaign_id = uuid::Uuid::parse_str(campaign_id_str).ok();
            Some((email.to_string(), campaign_id))
        } else {
            Some((payload.to_string(), None))
        }
    }

    /// Generate List-Unsubscribe header value
    pub fn generate_list_unsubscribe_header(
        &self,
        email: &str,
        campaign_id: Option<uuid::Uuid>,
        mailto_address: Option<&str>,
    ) -> String {
        let token = self.generate_unsubscribe_token(email, campaign_id);
        let https_url = format!("{}/{}", self.unsubscribe_base_url, token);

        if let Some(mailto) = mailto_address {
            format!(
                "<mailto:{}?subject=unsubscribe>, <{}>",
                mailto, https_url
            )
        } else {
            format!("<{}>", https_url)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_recipient() -> Recipient {
        Recipient {
            id: uuid::Uuid::new_v4(),
            recipient_list_id: uuid::Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: Some("John Doe".to_string()),
            status: "active".to_string(),
            attributes: serde_json::json!({
                "company": "Acme Corp",
                "plan": "premium"
            }),
            subscribed_at: chrono::Utc::now(),
            unsubscribed_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_render_basic_template() {
        let renderer = TemplateRenderer::new("https://mail.example.com/unsubscribe".to_string());
        let recipient = create_test_recipient();

        let template = "Hello {{name}}, your email is {{email}}";
        let result = renderer.render(template, &recipient, None);

        assert_eq!(result, "Hello John Doe, your email is test@example.com");
    }

    #[test]
    fn test_render_with_attributes() {
        let renderer = TemplateRenderer::new("https://mail.example.com/unsubscribe".to_string());
        let recipient = create_test_recipient();

        let template = "Welcome {{first_name}} from {{attributes.company}}!";
        let result = renderer.render(template, &recipient, None);

        assert_eq!(result, "Welcome John from Acme Corp!");
    }

    #[test]
    fn test_render_removes_unused() {
        let renderer = TemplateRenderer::new("https://mail.example.com/unsubscribe".to_string());
        let recipient = create_test_recipient();

        let template = "Hello {{name}}, {{unknown_var}} test";
        let result = renderer.render(template, &recipient, None);

        assert_eq!(result, "Hello John Doe,  test");
    }

    #[test]
    fn test_unsubscribe_token_roundtrip() {
        let renderer = TemplateRenderer::new("https://mail.example.com/unsubscribe".to_string());
        let email = "test@example.com";
        let campaign_id = Some(uuid::Uuid::new_v4());

        let token = renderer.generate_unsubscribe_token(email, campaign_id);
        let (parsed_email, parsed_campaign) = renderer.parse_unsubscribe_token(&token).unwrap();

        assert_eq!(parsed_email, email);
        assert_eq!(parsed_campaign, campaign_id);
    }
}
