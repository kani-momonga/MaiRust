//! Tenant handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use mairust_storage::{CreateTenant, Tenant, TenantRepository};
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use crate::auth::{require_scope, require_tenant_access, AppState, AuthContext};

/// List all tenants (admin only - requires 'admin:tenants' scope)
pub async fn list_tenants(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<Tenant>>, StatusCode> {
    // Require admin scope for listing all tenants
    require_scope(&auth, "admin:tenants")?;

    let repo = TenantRepository::new(state.db_pool.clone());

    let tenants = repo
        .find_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(tenants))
}

/// Get a tenant by ID
pub async fn get_tenant(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Tenant>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = TenantRepository::new(state.db_pool.clone());

    let tenant = repo
        .find_by_id(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(tenant))
}

/// Create a new tenant (admin only - requires 'admin:tenants' scope)
pub async fn create_tenant(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<CreateTenant>,
) -> Result<(StatusCode, Json<Tenant>), StatusCode> {
    // Require admin scope for creating tenants
    require_scope(&auth, "admin:tenants")?;

    let repo = TenantRepository::new(state.db_pool.clone());

    let tenant = repo
        .create(&input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(tenant)))
}

/// Delete a tenant (admin only - requires 'admin:tenants' scope)
pub async fn delete_tenant(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // Require admin scope for deleting tenants
    require_scope(&auth, "admin:tenants")?;

    // Also verify tenant access
    require_tenant_access(&auth, tenant_id)?;

    let repo = TenantRepository::new(state.db_pool.clone());

    // Verify tenant exists
    let _ = repo
        .find_by_id(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or_else(|| {
            warn!("Tenant {} not found", tenant_id);
            StatusCode::NOT_FOUND
        })?;

    repo.delete(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
