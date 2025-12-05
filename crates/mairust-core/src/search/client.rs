//! Meilisearch client implementation

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Meilisearch configuration
#[derive(Debug, Clone)]
pub struct MeilisearchConfig {
    /// Meilisearch server URL
    pub url: String,
    /// API key for authentication
    pub api_key: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Index name for messages
    pub messages_index: String,
}

impl Default for MeilisearchConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:7700".to_string(),
            api_key: None,
            timeout_secs: 30,
            messages_index: "messages".to_string(),
        }
    }
}

/// Search result from Meilisearch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult<T> {
    pub hits: Vec<T>,
    pub query: String,
    #[serde(rename = "processingTimeMs")]
    pub processing_time_ms: u64,
    #[serde(rename = "estimatedTotalHits")]
    pub estimated_total_hits: Option<u64>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

/// Search request parameters
#[derive(Debug, Clone, Serialize)]
pub struct SearchRequest {
    pub q: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "attributesToRetrieve")]
    pub attributes_to_retrieve: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "attributesToHighlight")]
    pub attributes_to_highlight: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<Vec<String>>,
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            q: String::new(),
            offset: None,
            limit: Some(20),
            filter: None,
            attributes_to_retrieve: None,
            attributes_to_highlight: Some(vec!["subject".to_string(), "body_preview".to_string()]),
            sort: Some(vec!["received_at:desc".to_string()]),
        }
    }
}

/// Task response from Meilisearch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    #[serde(rename = "taskUid")]
    pub task_uid: u64,
    #[serde(rename = "indexUid")]
    pub index_uid: Option<String>,
    pub status: String,
    #[serde(rename = "enqueuedAt")]
    pub enqueued_at: String,
}

/// Index settings for message search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSettings {
    #[serde(rename = "searchableAttributes")]
    pub searchable_attributes: Vec<String>,
    #[serde(rename = "filterableAttributes")]
    pub filterable_attributes: Vec<String>,
    #[serde(rename = "sortableAttributes")]
    pub sortable_attributes: Vec<String>,
    #[serde(rename = "displayedAttributes")]
    pub displayed_attributes: Vec<String>,
}

impl Default for IndexSettings {
    fn default() -> Self {
        Self {
            searchable_attributes: vec![
                "subject".to_string(),
                "body_preview".to_string(),
                "from_address".to_string(),
                "to_addresses".to_string(),
            ],
            filterable_attributes: vec![
                "tenant_id".to_string(),
                "mailbox_id".to_string(),
                "from_address".to_string(),
                "has_attachments".to_string(),
                "seen".to_string(),
                "flagged".to_string(),
                "received_at".to_string(),
                "tags".to_string(),
            ],
            sortable_attributes: vec!["received_at".to_string(), "subject".to_string()],
            displayed_attributes: vec!["*".to_string()],
        }
    }
}

/// Meilisearch client for full-text search
pub struct MeilisearchClient {
    config: MeilisearchConfig,
    client: Client,
}

impl MeilisearchClient {
    /// Create a new Meilisearch client
    pub fn new(config: MeilisearchConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    /// Get authorization header if API key is configured
    fn auth_header(&self) -> Option<(&'static str, String)> {
        self.config
            .api_key
            .as_ref()
            .map(|key| ("Authorization", format!("Bearer {}", key)))
    }

    /// Build a request with optional auth header
    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.config.url, path);
        let mut request = self.client.request(method, &url);

        if let Some((header, value)) = self.auth_header() {
            request = request.header(header, value);
        }

        request
    }

    /// Check if Meilisearch is healthy
    pub async fn health_check(&self) -> Result<bool, String> {
        let response = self
            .build_request(reqwest::Method::GET, "/health")
            .send()
            .await
            .map_err(|e| format!("Health check request failed: {}", e))?;

        let status = response.status();
        if status.is_success() {
            Ok(true)
        } else {
            let body = response.text().await.unwrap_or_else(|e| {
                warn!("Failed to read Meilisearch health response body: {}", e);
                String::new()
            });
            warn!(
                "Meilisearch health check failed: status={} body={}",
                status, body
            );
            Ok(false)
        }
    }

    /// Create or update the messages index with proper settings
    pub async fn setup_index(&self) -> Result<(), String> {
        let index_name = &self.config.messages_index;

        // Create index if it doesn't exist
        let create_body = serde_json::json!({
            "uid": index_name,
            "primaryKey": "id"
        });

        let response = self
            .build_request(reqwest::Method::POST, "/indexes")
            .json(&create_body)
            .send()
            .await
            .map_err(|e| format!("Failed to create index: {}", e))?;

        if response.status().is_success() || response.status() == 409 {
            // Index created or already exists
            debug!("Index {} ready", index_name);
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to create index: {} - {}", status, body);
            return Err(format!("Failed to create index: {}", status));
        }

        // Update index settings
        let settings = IndexSettings::default();
        let settings_path = format!("/indexes/{}/settings", index_name);

        let response = self
            .build_request(reqwest::Method::PATCH, &settings_path)
            .json(&settings)
            .send()
            .await
            .map_err(|e| format!("Failed to update index settings: {}", e))?;

        if response.status().is_success() {
            info!("Index {} settings updated", index_name);
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to update index settings: {} - {}", status, body);
            Err(format!("Failed to update index settings: {}", status))
        }
    }

    /// Add or update documents in the index
    pub async fn index_documents<T: Serialize>(
        &self,
        documents: &[T],
    ) -> Result<TaskResponse, String> {
        if documents.is_empty() {
            return Err("No documents to index".to_string());
        }

        let path = format!("/indexes/{}/documents", self.config.messages_index);

        let response = self
            .build_request(reqwest::Method::POST, &path)
            .json(documents)
            .send()
            .await
            .map_err(|e| format!("Failed to index documents: {}", e))?;

        if response.status().is_success() || response.status() == 202 {
            let task: TaskResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse task response: {}", e))?;
            debug!("Indexing task created: {}", task.task_uid);
            Ok(task)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to index documents: {} - {}", status, body);
            Err(format!("Failed to index documents: {}", status))
        }
    }

    /// Delete a document from the index
    pub async fn delete_document(&self, document_id: &str) -> Result<TaskResponse, String> {
        let path = format!(
            "/indexes/{}/documents/{}",
            self.config.messages_index, document_id
        );

        let response = self
            .build_request(reqwest::Method::DELETE, &path)
            .send()
            .await
            .map_err(|e| format!("Failed to delete document: {}", e))?;

        if response.status().is_success() || response.status() == 202 {
            let task: TaskResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse task response: {}", e))?;
            debug!("Delete task created: {}", task.task_uid);
            Ok(task)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to delete document: {} - {}", status, body);
            Err(format!("Failed to delete document: {}", status))
        }
    }

    /// Search for documents
    pub async fn search<T: for<'de> Deserialize<'de>>(
        &self,
        request: SearchRequest,
    ) -> Result<SearchResult<T>, String> {
        let path = format!("/indexes/{}/search", self.config.messages_index);

        let response = self
            .build_request(reqwest::Method::POST, &path)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Search failed: {}", e))?;

        if response.status().is_success() {
            let result: SearchResult<T> = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse search response: {}", e))?;
            debug!(
                "Search completed in {}ms, {} hits",
                result.processing_time_ms,
                result.hits.len()
            );
            Ok(result)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Search failed: {} - {}", status, body);
            Err(format!("Search failed: {}", status))
        }
    }

    /// Delete all documents matching a filter
    pub async fn delete_by_filter(&self, filter: &str) -> Result<TaskResponse, String> {
        let path = format!("/indexes/{}/documents/delete", self.config.messages_index);

        let body = serde_json::json!({
            "filter": filter
        });

        let response = self
            .build_request(reqwest::Method::POST, &path)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to delete by filter: {}", e))?;

        if response.status().is_success() || response.status() == 202 {
            let task: TaskResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse task response: {}", e))?;
            debug!("Delete by filter task created: {}", task.task_uid);
            Ok(task)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to delete by filter: {} - {}", status, body);
            Err(format!("Failed to delete by filter: {}", status))
        }
    }

    /// Get task status
    pub async fn get_task(&self, task_uid: u64) -> Result<serde_json::Value, String> {
        let path = format!("/tasks/{}", task_uid);

        let response = self
            .build_request(reqwest::Method::GET, &path)
            .send()
            .await
            .map_err(|e| format!("Failed to get task: {}", e))?;

        if response.status().is_success() {
            let task: serde_json::Value = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse task: {}", e))?;
            Ok(task)
        } else {
            let status = response.status();
            Err(format!("Failed to get task: {}", status))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MeilisearchConfig::default();
        assert_eq!(config.url, "http://localhost:7700");
        assert_eq!(config.messages_index, "messages");
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_search_request_serialization() {
        let request = SearchRequest {
            q: "test query".to_string(),
            limit: Some(10),
            filter: Some("tenant_id = 'abc'".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["q"], "test query");
        assert_eq!(json["limit"], 10);
        assert_eq!(json["filter"], "tenant_id = 'abc'");
    }

    #[test]
    fn test_index_settings_default() {
        let settings = IndexSettings::default();
        assert!(settings
            .searchable_attributes
            .contains(&"subject".to_string()));
        assert!(settings
            .filterable_attributes
            .contains(&"tenant_id".to_string()));
        assert!(settings
            .sortable_attributes
            .contains(&"received_at".to_string()));
    }
}
