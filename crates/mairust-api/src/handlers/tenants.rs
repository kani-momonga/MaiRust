//! Tenant handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use mairust_storage::{CreateTenant, Tenant, TenantRepository};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AppState;

/// List all tenants (admin only)
pub async fn list_tenants(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Tenant>>, StatusCode> {
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
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Tenant>, StatusCode> {
    let repo = TenantRepository::new(state.db_pool.clone());

    let tenant = repo
        .find_by_id(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(tenant))
}

/// Create a new tenant
pub async fn create_tenant(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateTenant>,
) -> Result<(StatusCode, Json<Tenant>), StatusCode> {
    let repo = TenantRepository::new(state.db_pool.clone());

    let tenant = repo
        .create(&input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(tenant)))
}

/// Delete a tenant
pub async fn delete_tenant(
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let repo = TenantRepository::new(state.db_pool.clone());

    repo.delete(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
