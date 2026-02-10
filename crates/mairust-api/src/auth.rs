//! Authentication module

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use mairust_common::types::{TenantId, UserId};
use mairust_storage::repository::api_keys::ApiKey;
use mairust_storage::{ApiKeyRepository, ApiKeyRepositoryTrait, DatabasePool};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db_pool: DatabasePool,
}

/// Authenticated context extracted from API key
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// The tenant ID this API key belongs to
    pub tenant_id: TenantId,
    /// The user ID this API key belongs to (if any)
    pub user_id: Option<UserId>,
    /// Scopes granted to this API key
    pub scopes: Vec<String>,
    /// API key ID for audit logging
    pub api_key_id: Uuid,
}

impl AuthContext {
    /// Check if the authenticated context has a specific scope
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(&"*".to_string()) || self.scopes.contains(&scope.to_string())
    }

    /// Check if the request is authorized for the given tenant
    pub fn is_authorized_for_tenant(&self, tenant_id: TenantId) -> bool {
        self.tenant_id == tenant_id
    }
}

/// Extract API key from request
pub fn extract_api_key(req: &Request) -> Option<&str> {
    // Check Authorization header
    if let Some(auth) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Bearer ") {
                return Some(&auth_str[7..]);
            }
        }
    }

    // Check X-API-Key header
    if let Some(key) = req.headers().get("x-api-key") {
        if let Ok(key_str) = key.to_str() {
            return Some(key_str);
        }
    }

    None
}

/// Extract the prefix from an API key (first 8 characters)
fn extract_key_prefix(api_key: &str) -> Option<&str> {
    if api_key.len() >= 8 {
        Some(&api_key[..8])
    } else {
        None
    }
}

/// Hash an API key for comparison
fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Verify an API key against a stored hash.
///
/// Supports both modern Argon2 hashes (`$argon2...`) and legacy SHA-256 hex hashes
/// for backward compatibility during migration.
fn verify_api_key(api_key: &str, stored_hash: &str) -> bool {
    if stored_hash.starts_with("$argon2") {
        return PasswordHash::new(stored_hash)
            .ok()
            .and_then(|parsed_hash| {
                Argon2::default()
                    .verify_password(api_key.as_bytes(), &parsed_hash)
                    .ok()
            })
            .is_some();
    }

    hash_api_key(api_key) == stored_hash
}

/// Validate an API key against the database
async fn validate_api_key(db_pool: &DatabasePool, api_key: &str) -> Result<ApiKey, StatusCode> {
    let prefix = extract_key_prefix(api_key).ok_or_else(|| {
        warn!("API key too short");
        StatusCode::UNAUTHORIZED
    })?;

    let repo = ApiKeyRepository::new(db_pool.clone());

    // Find potential matches by prefix
    let candidates = repo.find_by_prefix(prefix).await.map_err(|e| {
        error!("Database error while looking up API key: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if candidates.is_empty() {
        warn!("No API key found with prefix: {}", prefix);
        return Err(StatusCode::UNAUTHORIZED);
    }

    for candidate in candidates {
        if verify_api_key(api_key, &candidate.key_hash) {
            // Check expiration
            if candidate.is_expired() {
                warn!("API key {} has expired", candidate.id);
                return Err(StatusCode::UNAUTHORIZED);
            }

            // Update last_used_at (fire and forget, don't fail auth on this)
            let repo_clone = ApiKeyRepository::new(db_pool.clone());
            let key_id = candidate.id;
            tokio::spawn(async move {
                if let Err(e) = repo_clone.update_last_used(key_id).await {
                    error!("Failed to update API key last_used_at: {}", e);
                }
            });

            debug!(
                "API key {} authenticated for tenant {}",
                candidate.id, candidate.tenant_id
            );
            return Ok(candidate);
        }
    }

    warn!("API key hash mismatch for prefix: {}", prefix);
    Err(StatusCode::UNAUTHORIZED)
}

#[cfg(test)]
mod tests {
    use super::verify_api_key;
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    use sha2::{Digest, Sha256};

    #[test]
    fn verifies_legacy_sha256_hash() {
        let api_key = "mk_test_legacy_key";
        let mut hasher = Sha256::new();
        hasher.update(api_key.as_bytes());
        let legacy_hash = hex::encode(hasher.finalize());

        assert!(verify_api_key(api_key, &legacy_hash));
        assert!(!verify_api_key("wrong_key", &legacy_hash));
    }

    #[test]
    fn verifies_argon2_hash() {
        let api_key = "mk_test_argon2_key";
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(api_key.as_bytes(), &salt)
            .expect("argon2 hash generation should succeed")
            .to_string();

        assert!(verify_api_key(api_key, &hash));
        assert!(!verify_api_key("wrong_key", &hash));
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth for health check endpoints
    if request.uri().path().starts_with("/health") {
        return Ok(next.run(request).await);
    }

    // Extract API key
    let api_key = extract_api_key(&request).ok_or_else(|| {
        warn!("Missing API key in request to {}", request.uri().path());
        StatusCode::UNAUTHORIZED
    })?;

    // Validate API key against database
    let validated_key = validate_api_key(&state.db_pool, api_key).await?;

    // Create auth context
    let auth_context = AuthContext {
        tenant_id: validated_key.tenant_id,
        user_id: validated_key.user_id,
        scopes: validated_key.scopes_vec(),
        api_key_id: validated_key.id,
    };

    // Store auth context in request extensions
    request.extensions_mut().insert(auth_context);

    Ok(next.run(request).await)
}

/// Helper function to extract AuthContext from request extensions
pub fn get_auth_context(req: &Request) -> Option<&AuthContext> {
    req.extensions().get::<AuthContext>()
}

/// Check if the authenticated user is authorized for a specific tenant
/// Returns an error if not authorized
pub fn require_tenant_access(
    auth_context: &AuthContext,
    tenant_id: TenantId,
) -> Result<(), StatusCode> {
    if !auth_context.is_authorized_for_tenant(tenant_id) {
        warn!(
            "Tenant access denied: API key tenant {} tried to access tenant {}",
            auth_context.tenant_id, tenant_id
        );
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// Check if the authenticated user has a specific scope
/// Returns an error if scope is missing
pub fn require_scope(auth_context: &AuthContext, scope: &str) -> Result<(), StatusCode> {
    if !auth_context.has_scope(scope) {
        warn!(
            "Scope access denied: API key {} lacks scope '{}'",
            auth_context.api_key_id, scope
        );
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}
