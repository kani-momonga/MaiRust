//! Hook handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use mairust_common::types::HookType;
use mairust_storage::repository::hooks::CreateHook;
use mairust_storage::{Hook, HookRepository, HookRepositoryTrait};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, warn};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// Query parameters for listing hooks
#[derive(Debug, Clone, Deserialize)]
pub struct ListHooksQuery {
    pub hook_type: Option<String>,
    pub enabled_only: Option<bool>,
}

/// Request body for creating a hook
#[derive(Debug, Clone, Deserialize)]
pub struct CreateHookRequest {
    pub name: String,
    pub hook_type: String,
    pub plugin_id: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_timeout")]
    pub timeout_ms: i32,
    #[serde(default = "default_on_timeout")]
    pub on_timeout: String,
    #[serde(default = "default_on_error")]
    pub on_error: String,
    #[serde(default)]
    pub filter_config: serde_json::Value,
    #[serde(default)]
    pub config: serde_json::Value,
}

fn default_priority() -> i32 {
    100
}

fn default_timeout() -> i32 {
    5000
}

fn default_on_timeout() -> String {
    "continue".to_string()
}

fn default_on_error() -> String {
    "continue".to_string()
}

/// Hook response with additional info
#[derive(Debug, Clone, Serialize)]
pub struct HookResponse {
    #[serde(flatten)]
    pub hook: Hook,
}

impl From<Hook> for HookResponse {
    fn from(hook: Hook) -> Self {
        Self { hook }
    }
}

/// Parse hook type from string
fn parse_hook_type(s: &str) -> Option<HookType> {
    match s.to_lowercase().as_str() {
        "pre_receive" => Some(HookType::PreReceive),
        "post_receive" => Some(HookType::PostReceive),
        "pre_send" => Some(HookType::PreSend),
        "pre_delivery" => Some(HookType::PreDelivery),
        _ => None,
    }
}

/// List hooks for a tenant
pub async fn list_hooks(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<ListHooksQuery>,
) -> Result<Json<Vec<HookResponse>>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = HookRepository::new(state.db_pool.clone());

    // Always filter by tenant - never return hooks from other tenants
    let hooks = repo
        .list(Some(tenant_id))
        .await
        .map_err(|e| {
            error!("Database error while listing hooks: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Apply additional filters in-memory if needed
    let filtered_hooks: Vec<Hook> = if let Some(hook_type_str) = query.hook_type {
        let hook_type = parse_hook_type(&hook_type_str).ok_or(StatusCode::BAD_REQUEST)?;

        hooks
            .into_iter()
            .filter(|h| h.hook_type == hook_type.to_string())
            .filter(|h| !query.enabled_only.unwrap_or(false) || h.enabled)
            .collect()
    } else if query.enabled_only.unwrap_or(false) {
        hooks.into_iter().filter(|h| h.enabled).collect()
    } else {
        hooks
    };

    let responses: Vec<HookResponse> = filtered_hooks.into_iter().map(Into::into).collect();

    Ok(Json(responses))
}

/// Get a hook by ID
pub async fn get_hook(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, hook_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<HookResponse>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = HookRepository::new(state.db_pool.clone());

    let hook = repo
        .get(hook_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching hook: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Verify hook belongs to the requested tenant
    if hook.tenant_id != Some(tenant_id) {
        warn!(
            "Hook {} does not belong to tenant {}",
            hook_id, tenant_id
        );
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(hook.into()))
}

/// Create a new hook
pub async fn create_hook(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreateHookRequest>,
) -> Result<(StatusCode, Json<HookResponse>), StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = HookRepository::new(state.db_pool.clone());

    let hook_type = parse_hook_type(&input.hook_type).ok_or(StatusCode::BAD_REQUEST)?;

    let create_input = CreateHook {
        tenant_id: Some(tenant_id),
        name: input.name,
        hook_type,
        plugin_id: input.plugin_id,
        priority: input.priority,
        timeout_ms: input.timeout_ms,
        on_timeout: input.on_timeout,
        on_error: input.on_error,
        filter_config: input.filter_config,
        config: input.config,
    };

    let hook = repo.create(create_input).await.map_err(|e| {
        error!("Database error while creating hook: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(hook.into())))
}

/// Enable a hook
pub async fn enable_hook(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, hook_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<HookResponse>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = HookRepository::new(state.db_pool.clone());

    // Check hook exists and belongs to tenant
    let hook = repo
        .get(hook_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching hook: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Verify hook belongs to the requested tenant
    if hook.tenant_id != Some(tenant_id) {
        warn!(
            "Hook {} does not belong to tenant {}",
            hook_id, tenant_id
        );
        return Err(StatusCode::NOT_FOUND);
    }

    repo.enable(hook_id).await.map_err(|e| {
        error!("Database error while enabling hook: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let hook = repo
        .get(hook_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching hook: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(hook.into()))
}

/// Disable a hook
pub async fn disable_hook(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, hook_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<HookResponse>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = HookRepository::new(state.db_pool.clone());

    // Check hook exists and belongs to tenant
    let hook = repo
        .get(hook_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching hook: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Verify hook belongs to the requested tenant
    if hook.tenant_id != Some(tenant_id) {
        warn!(
            "Hook {} does not belong to tenant {}",
            hook_id, tenant_id
        );
        return Err(StatusCode::NOT_FOUND);
    }

    repo.disable(hook_id).await.map_err(|e| {
        error!("Database error while disabling hook: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let hook = repo
        .get(hook_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching hook: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(hook.into()))
}

/// Delete a hook
pub async fn delete_hook(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, hook_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = HookRepository::new(state.db_pool.clone());

    // Check hook exists and belongs to tenant
    let hook = repo
        .get(hook_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching hook: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Verify hook belongs to the requested tenant
    if hook.tenant_id != Some(tenant_id) {
        warn!(
            "Hook {} does not belong to tenant {}",
            hook_id, tenant_id
        );
        return Err(StatusCode::NOT_FOUND);
    }

    repo.delete(hook_id).await.map_err(|e| {
        error!("Database error while deleting hook: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}
