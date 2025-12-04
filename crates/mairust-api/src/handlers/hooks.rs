//! Hook handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use mairust_common::types::HookType;
use mairust_storage::{Hook, HookRepository, HookRepositoryTrait};
use mairust_storage::repository::hooks::CreateHook;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AppState;

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
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<ListHooksQuery>,
) -> Result<Json<Vec<HookResponse>>, StatusCode> {
    let repo = HookRepository::new(state.db_pool.clone());

    let hooks = if let Some(hook_type_str) = query.hook_type {
        let hook_type = parse_hook_type(&hook_type_str)
            .ok_or(StatusCode::BAD_REQUEST)?;

        if query.enabled_only.unwrap_or(false) {
            repo.list_enabled_by_type(hook_type)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        } else {
            repo.list_by_type(hook_type)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
    } else {
        repo.list(Some(tenant_id))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let responses: Vec<HookResponse> = hooks.into_iter().map(Into::into).collect();

    Ok(Json(responses))
}

/// Get a hook by ID
pub async fn get_hook(
    State(state): State<Arc<AppState>>,
    Path((_tenant_id, hook_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<HookResponse>, StatusCode> {
    let repo = HookRepository::new(state.db_pool.clone());

    let hook = repo
        .get(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(hook.into()))
}

/// Create a new hook
pub async fn create_hook(
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreateHookRequest>,
) -> Result<(StatusCode, Json<HookResponse>), StatusCode> {
    let repo = HookRepository::new(state.db_pool.clone());

    let hook_type = parse_hook_type(&input.hook_type)
        .ok_or(StatusCode::BAD_REQUEST)?;

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

    let hook = repo
        .create(create_input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(hook.into())))
}

/// Enable a hook
pub async fn enable_hook(
    State(state): State<Arc<AppState>>,
    Path((_tenant_id, hook_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<HookResponse>, StatusCode> {
    let repo = HookRepository::new(state.db_pool.clone());

    // Check hook exists
    let _ = repo
        .get(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.enable(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let hook = repo
        .get(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(hook.into()))
}

/// Disable a hook
pub async fn disable_hook(
    State(state): State<Arc<AppState>>,
    Path((_tenant_id, hook_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<HookResponse>, StatusCode> {
    let repo = HookRepository::new(state.db_pool.clone());

    // Check hook exists
    let _ = repo
        .get(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.disable(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let hook = repo
        .get(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(hook.into()))
}

/// Delete a hook
pub async fn delete_hook(
    State(state): State<Arc<AppState>>,
    Path((_tenant_id, hook_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let repo = HookRepository::new(state.db_pool.clone());

    // Check hook exists
    let _ = repo
        .get(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.delete(hook_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
