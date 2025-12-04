//! Authentication module

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

/// Validate an API key against the database
async fn validate_api_key(
    db_pool: &DatabasePool,
    api_key: &str,
) -> Result<ApiKey, StatusCode> {
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

    // Hash the provided key and compare
    let key_hash = hash_api_key(api_key);

    for candidate in candidates {
        if candidate.key_hash == key_hash {
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

            debug!("API key {} authenticated for tenant {}", candidate.id, candidate.tenant_id);
            return Ok(candidate);
        }
    }

    warn!("API key hash mismatch for prefix: {}", prefix);
    Err(StatusCode::UNAUTHORIZED)
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
pub fn require_tenant_access(auth_context: &AuthContext, tenant_id: TenantId) -> Result<(), StatusCode> {
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
