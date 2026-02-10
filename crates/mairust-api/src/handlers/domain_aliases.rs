//! Domain alias handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use mairust_storage::{CreateDomainAlias, DomainAlias, DomainAliasRepository, DomainAliasRepositoryTrait, DomainRepository, DomainRepositoryTrait};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// Request body for creating a domain alias
#[derive(Debug, Clone, Deserialize)]
pub struct CreateDomainAliasRequest {
    pub alias_domain: String,
    pub primary_domain_id: Uuid,
}

/// Response for domain alias with primary domain info
#[derive(Debug, Clone, Serialize)]
pub struct DomainAliasResponse {
    #[serde(flatten)]
    pub alias: DomainAlias,
    pub primary_domain_name: Option<String>,
}

/// List domain aliases for a tenant
pub async fn list_domain_aliases(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<DomainAliasResponse>>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let alias_repo = DomainAliasRepository::new(state.db_pool.clone());
    let domain_repo = DomainRepository::new(state.db_pool.clone());

    let aliases = alias_repo.list(tenant_id).await.map_err(|e| {
        error!("Database error while listing domain aliases: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut responses = Vec::with_capacity(aliases.len());
    for alias in aliases {
        let primary_domain_name = domain_repo
            .get(tenant_id, alias.primary_domain_id)
            .await
            .ok()
            .flatten()
            .map(|d| d.name);
        responses.push(DomainAliasResponse {
            alias,
            primary_domain_name,
        });
    }

    Ok(Json(responses))
}

/// Get a domain alias by ID
pub async fn get_domain_alias(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, alias_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DomainAliasResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let alias_repo = DomainAliasRepository::new(state.db_pool.clone());
    let domain_repo = DomainRepository::new(state.db_pool.clone());

    let alias = alias_repo
        .get(tenant_id, alias_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain alias: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!("Domain alias {} not found for tenant {}", alias_id, tenant_id);
            StatusCode::NOT_FOUND
        })?;

    let primary_domain_name = domain_repo
        .get(tenant_id, alias.primary_domain_id)
        .await
        .ok()
        .flatten()
        .map(|d| d.name);

    Ok(Json(DomainAliasResponse {
        alias,
        primary_domain_name,
    }))
}

/// Create a new domain alias
pub async fn create_domain_alias(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreateDomainAliasRequest>,
) -> Result<(StatusCode, Json<DomainAliasResponse>), StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    // Validate alias domain name
    if !is_valid_domain_name(&input.alias_domain) {
        warn!("Invalid alias domain name format: {}", input.alias_domain);
        return Err(StatusCode::BAD_REQUEST);
    }

    let alias_repo = DomainAliasRepository::new(state.db_pool.clone());
    let domain_repo = DomainRepository::new(state.db_pool.clone());

    // Verify primary domain exists and belongs to tenant
    let primary_domain = domain_repo
        .get(tenant_id, input.primary_domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching primary domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Primary domain {} not found for tenant {}",
                input.primary_domain_id, tenant_id
            );
            StatusCode::BAD_REQUEST
        })?;

    // Check if alias domain already exists as a domain (global uniqueness check)
    if let Ok(Some(_)) = domain_repo.find_by_name(&input.alias_domain).await {
        warn!("Domain {} already exists", input.alias_domain);
        return Err(StatusCode::CONFLICT);
    }

    if let Ok(Some(_)) = alias_repo.get_by_alias_domain(&input.alias_domain).await {
        warn!("Domain alias {} already exists", input.alias_domain);
        return Err(StatusCode::CONFLICT);
    }

    let create_input = CreateDomainAlias {
        tenant_id,
        alias_domain: input.alias_domain.to_lowercase(),
        primary_domain_id: input.primary_domain_id,
    };

    let alias = alias_repo.create(create_input).await.map_err(|e| {
        error!("Database error while creating domain alias: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!(
        "Created domain alias {} -> {}",
        alias.alias_domain, primary_domain.name
    );

    Ok((
        StatusCode::CREATED,
        Json(DomainAliasResponse {
            alias,
            primary_domain_name: Some(primary_domain.name),
        }),
    ))
}

/// Enable a domain alias
pub async fn enable_domain_alias(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, alias_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = DomainAliasRepository::new(state.db_pool.clone());

    // Verify alias exists and belongs to tenant
    let _ = repo
        .get(tenant_id, alias_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain alias: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.enable(alias_id).await.map_err(|e| {
        error!("Database error while enabling domain alias: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Enabled domain alias {}", alias_id);
    Ok(StatusCode::OK)
}

/// Disable a domain alias
pub async fn disable_domain_alias(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, alias_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = DomainAliasRepository::new(state.db_pool.clone());

    // Verify alias exists and belongs to tenant
    let _ = repo
        .get(tenant_id, alias_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain alias: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.disable(alias_id).await.map_err(|e| {
        error!("Database error while disabling domain alias: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Disabled domain alias {}", alias_id);
    Ok(StatusCode::OK)
}

/// Delete a domain alias
pub async fn delete_domain_alias(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, alias_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = DomainAliasRepository::new(state.db_pool.clone());

    // Verify alias exists and belongs to tenant
    let alias = repo
        .get(tenant_id, alias_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain alias: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.delete(alias_id).await.map_err(|e| {
        error!("Database error while deleting domain alias: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Deleted domain alias {}", alias.alias_domain);
    Ok(StatusCode::NO_CONTENT)
}

/// Validate domain name format
fn is_valid_domain_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 {
        return false;
    }

    let labels: Vec<&str> = name.split('.').collect();
    if labels.is_empty() {
        return false;
    }

    for label in labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }

        let chars: Vec<char> = label.chars().collect();
        if !chars.first().map(|c| c.is_alphanumeric()).unwrap_or(false) {
            return false;
        }
        if !chars.last().map(|c| c.is_alphanumeric()).unwrap_or(false) {
            return false;
        }

        if !label.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return false;
        }
    }

    true
}
