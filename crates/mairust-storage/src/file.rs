//! File storage abstraction

use async_trait::async_trait;
use mairust_common::config::StorageConfig;
use mairust_common::{Error, Result};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info};

/// File storage trait
#[async_trait]
pub trait FileStorage: Send + Sync {
    /// Store a file and return its path
    async fn store(&self, path: &str, data: &[u8]) -> Result<String>;

    /// Read a file
    async fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// Retrieve a file (alias for read)
    async fn retrieve(&self, path: &str) -> Result<Vec<u8>> {
        self.read(path).await
    }

    /// Delete a file
    async fn delete(&self, path: &str) -> Result<()>;

    /// Check if a file exists
    async fn exists(&self, path: &str) -> Result<bool>;

    /// Get file size
    async fn size(&self, path: &str) -> Result<u64>;
}

/// Local filesystem storage
pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    /// Create a new local storage instance from config
    pub fn new(config: &StorageConfig) -> Result<Self> {
        Self::from_path(&config.path)
    }

    /// Create a new local storage instance from a path string
    pub fn from_path_str(path: &str) -> Result<Self> {
        Self::from_path(std::path::Path::new(path))
    }

    /// Create a new local storage instance from a path
    pub fn from_path(path: &std::path::Path) -> Result<Self> {
        // Ensure base directory exists
        std::fs::create_dir_all(path)
            .map_err(|e| Error::Storage(format!("Failed to create storage directory: {}", e)))?;

        info!(path = %path.display(), "Initialized local file storage");

        Ok(Self {
            base_path: path.to_path_buf(),
        })
    }

    /// Get full path for a relative path, with path traversal protection
    fn full_path(&self, path: &str) -> std::result::Result<PathBuf, Error> {
        // Reject paths containing traversal sequences
        if path.contains("..") {
            return Err(Error::Storage(
                "Path traversal detected: '..' is not allowed".to_string(),
            ));
        }

        // Reject absolute paths
        if path.starts_with('/') || path.starts_with('\\') {
            return Err(Error::Storage(
                "Absolute paths are not allowed".to_string(),
            ));
        }

        let full = self.base_path.join(path);

        // Canonicalize and verify the path stays within base_path
        // For new files, check the parent directory
        let canonical_base = self
            .base_path
            .canonicalize()
            .map_err(|e| Error::Storage(format!("Failed to canonicalize base path: {}", e)))?;

        // If the full path exists, canonicalize it directly
        // Otherwise, canonicalize the parent and append the filename
        let canonical_full = if full.exists() {
            full.canonicalize()
                .map_err(|e| Error::Storage(format!("Failed to canonicalize path: {}", e)))?
        } else if let Some(parent) = full.parent() {
            if parent.exists() {
                let canonical_parent = parent.canonicalize().map_err(|e| {
                    Error::Storage(format!("Failed to canonicalize parent path: {}", e))
                })?;
                if let Some(filename) = full.file_name() {
                    canonical_parent.join(filename)
                } else {
                    return Err(Error::Storage("Invalid file path".to_string()));
                }
            } else {
                // Parent doesn't exist yet (will be created), verify path components
                full.clone()
            }
        } else {
            full.clone()
        };

        if !canonical_full.starts_with(&canonical_base) {
            return Err(Error::Storage(
                "Path traversal detected: resolved path is outside storage directory".to_string(),
            ));
        }

        Ok(full)
    }

    /// Ensure parent directory exists
    async fn ensure_parent_exists(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Storage(format!("Failed to create directory: {}", e)))?;
        }
        Ok(())
    }
}

#[async_trait]
impl FileStorage for LocalStorage {
    async fn store(&self, path: &str, data: &[u8]) -> Result<String> {
        let full_path = self.full_path(path)?;
        self.ensure_parent_exists(&full_path).await?;

        let mut file = fs::File::create(&full_path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create file: {}", e)))?;

        file.write_all(data)
            .await
            .map_err(|e| Error::Storage(format!("Failed to write file: {}", e)))?;

        debug!(path = %path, size = data.len(), "Stored file");

        Ok(path.to_string())
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = self.full_path(path)?;

        let mut file = fs::File::open(&full_path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to open file: {}", e)))?;

        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .await
            .map_err(|e| Error::Storage(format!("Failed to read file: {}", e)))?;

        debug!(path = %path, size = data.len(), "Read file");

        Ok(data)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.full_path(path)?;

        fs::remove_file(&full_path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to delete file: {}", e)))?;

        debug!(path = %path, "Deleted file");

        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let full_path = self.full_path(path)?;
        Ok(full_path.exists())
    }

    async fn size(&self, path: &str) -> Result<u64> {
        let full_path = self.full_path(path)?;

        let metadata = fs::metadata(&full_path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to get file metadata: {}", e)))?;

        Ok(metadata.len())
    }
}

/// Message storage helper
pub struct MessageStorage {
    storage: Box<dyn FileStorage>,
}

impl MessageStorage {
    /// Create a new message storage
    pub fn new(storage: Box<dyn FileStorage>) -> Self {
        Self { storage }
    }

    /// Generate storage path for a message
    pub fn generate_path(
        tenant_id: &uuid::Uuid,
        mailbox_id: &uuid::Uuid,
        message_id: &uuid::Uuid,
    ) -> String {
        format!(
            "{}/{}/{}.eml",
            tenant_id,
            mailbox_id,
            message_id
        )
    }

    /// Store a message
    pub async fn store_message(
        &self,
        tenant_id: &uuid::Uuid,
        mailbox_id: &uuid::Uuid,
        message_id: &uuid::Uuid,
        data: &[u8],
    ) -> Result<String> {
        let path = Self::generate_path(tenant_id, mailbox_id, message_id);
        self.storage.store(&path, data).await
    }

    /// Read a message
    pub async fn read_message(&self, path: &str) -> Result<Vec<u8>> {
        self.storage.read(path).await
    }

    /// Delete a message
    pub async fn delete_message(&self, path: &str) -> Result<()> {
        self.storage.delete(path).await
    }
}

/// Create file storage from configuration
pub fn create_storage(config: &StorageConfig) -> Result<Box<dyn FileStorage>> {
    match config.backend.as_str() {
        "fs" => Ok(Box::new(LocalStorage::new(config)?)),
        "s3" => {
            // S3 storage will be implemented in Phase 2
            Err(Error::Config("S3 storage not yet implemented".to_string()))
        }
        other => Err(Error::Config(format!(
            "Unsupported storage backend: {}",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_storage() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            backend: "fs".to_string(),
            path: temp_dir.path().to_path_buf(),
            s3: None,
        };

        let storage = LocalStorage::new(&config).unwrap();

        // Test store
        let data = b"Hello, World!";
        let path = storage.store("test/message.eml", data).await.unwrap();
        assert_eq!(path, "test/message.eml");

        // Test exists
        assert!(storage.exists("test/message.eml").await.unwrap());
        assert!(!storage.exists("nonexistent.eml").await.unwrap());

        // Test read
        let read_data = storage.read("test/message.eml").await.unwrap();
        assert_eq!(read_data, data);

        // Test size
        let size = storage.size("test/message.eml").await.unwrap();
        assert_eq!(size, data.len() as u64);

        // Test delete
        storage.delete("test/message.eml").await.unwrap();
        assert!(!storage.exists("test/message.eml").await.unwrap());
    }

    #[tokio::test]
    async fn test_path_traversal_prevention() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            backend: "fs".to_string(),
            path: temp_dir.path().to_path_buf(),
            s3: None,
        };

        let storage = LocalStorage::new(&config).unwrap();

        // Path traversal with .. should be rejected
        assert!(storage.store("../../../etc/passwd", b"evil").await.is_err());
        assert!(storage.read("../../../etc/passwd").await.is_err());
        assert!(storage.delete("../../sensitive").await.is_err());
        assert!(storage.exists("../outside").await.is_err());

        // Absolute paths should be rejected
        assert!(storage.store("/etc/passwd", b"evil").await.is_err());
        assert!(storage.read("/etc/shadow").await.is_err());

        // Normal paths should work
        assert!(storage.store("safe/path/file.eml", b"ok").await.is_ok());
    }
}
