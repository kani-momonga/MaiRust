//! User handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use mairust_storage::{CreateUser, User, UserRepository};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AppState;

/// List users in a tenant
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<User>>, StatusCode> {
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
    Path(user_id): Path<Uuid>,
) -> Result<Json<User>, StatusCode> {
    let repo = UserRepository::new(state.db_pool.clone());

    let user = repo
        .find_by_id(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(user))
}

/// Create a new user
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateUser>,
) -> Result<(StatusCode, Json<User>), StatusCode> {
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
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let repo = UserRepository::new(state.db_pool.clone());

    repo.delete(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
