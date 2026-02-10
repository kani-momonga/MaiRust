//! User handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use mairust_storage::{CreateUser, User, UserRepository};
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// List users in a tenant
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<User>>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = UserRepository::new(state.db_pool.clone());

    let users = repo
        .find_by_tenant(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(users))
}

/// Get a user by ID
pub async fn get_user(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<User>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = UserRepository::new(state.db_pool.clone());

    let user = repo
        .find_by_id(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter(|u| u.tenant_id == tenant_id)
        .ok_or_else(|| {
            warn!(
                "User {} not found or not owned by tenant {}",
                user_id, tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    Ok(Json(user))
}

/// Create a new user
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(mut input): Json<CreateUser>,
) -> Result<(StatusCode, Json<User>), StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    // Ensure the user is created for the correct tenant
    input.tenant_id = tenant_id;

    let repo = UserRepository::new(state.db_pool.clone());

    let user = repo
        .create(&input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(user)))
}

/// Delete a user
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = UserRepository::new(state.db_pool.clone());

    // Verify user exists and belongs to this tenant
    let _ = repo
        .find_by_id(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter(|u| u.tenant_id == tenant_id)
        .ok_or_else(|| {
            warn!(
                "User {} not found or not owned by tenant {}",
                user_id, tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    repo.delete(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
