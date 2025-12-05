//! Policy handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use mairust_storage::{CreatePolicyRule, PolicyRepository, PolicyRepositoryTrait, PolicyRule};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// Request body for creating a policy rule
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePolicyRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub priority: Option<i32>,
    pub domain_id: Option<Uuid>,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
}

/// Request body for updating a policy rule
#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePolicyRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub policy_type: Option<String>,
    pub priority: Option<i32>,
    pub conditions: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
}

/// Response for policy rule
#[derive(Debug, Clone, Serialize)]
pub struct PolicyRuleResponse {
    #[serde(flatten)]
    pub rule: PolicyRule,
}

/// List all policy rules for a tenant
pub async fn list_policies(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<PolicyRule>>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = PolicyRepository::new(state.db_pool.clone());

    let policies = repo.list_by_tenant(tenant_id).await.map_err(|e| {
        error!("Database error while listing policies: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(policies))
}

/// List effective policies for a tenant/domain combination
pub async fn list_effective_policies(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Option<Uuid>)>,
) -> Result<Json<Vec<PolicyRule>>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = PolicyRepository::new(state.db_pool.clone());

    let policies = repo.list_effective(tenant_id, domain_id).await.map_err(|e| {
        error!("Database error while listing effective policies: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(policies))
}

/// Get a policy rule by ID
pub async fn get_policy(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, policy_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<PolicyRuleResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = PolicyRepository::new(state.db_pool.clone());

    let policy = repo
        .get(policy_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching policy: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!("Policy {} not found", policy_id);
            StatusCode::NOT_FOUND
        })?;

    // Verify policy belongs to this tenant
    if policy.tenant_id != Some(tenant_id) && policy.tenant_id.is_some() {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(PolicyRuleResponse { rule: policy }))
}

/// Create a new policy rule
pub async fn create_policy(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreatePolicyRuleRequest>,
) -> Result<(StatusCode, Json<PolicyRuleResponse>), StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    // Validate policy type
    if !is_valid_policy_type(&input.policy_type) {
        warn!("Invalid policy type: {}", input.policy_type);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate conditions and actions format
    if let Err(e) = validate_conditions(&input.conditions) {
        warn!("Invalid conditions: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }

    if let Err(e) = validate_actions(&input.actions) {
        warn!("Invalid actions: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }

    let repo = PolicyRepository::new(state.db_pool.clone());

    let create_input = CreatePolicyRule {
        tenant_id: Some(tenant_id),
        domain_id: input.domain_id,
        name: input.name,
        description: input.description,
        policy_type: input.policy_type,
        priority: input.priority.unwrap_or(100),
        conditions: input.conditions,
        actions: input.actions,
    };

    let policy = repo.create(create_input).await.map_err(|e| {
        error!("Database error while creating policy: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Created policy rule: {}", policy.name);

    Ok((StatusCode::CREATED, Json(PolicyRuleResponse { rule: policy })))
}

/// Update a policy rule
pub async fn update_policy(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, policy_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdatePolicyRuleRequest>,
) -> Result<Json<PolicyRuleResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = PolicyRepository::new(state.db_pool.clone());

    // Get existing policy
    let existing = repo
        .get(policy_id)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Verify ownership
    if existing.tenant_id != Some(tenant_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Validate if policy type is being updated
    if let Some(ref policy_type) = input.policy_type {
        if !is_valid_policy_type(policy_type) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Validate conditions if provided
    if let Some(ref conditions) = input.conditions {
        if let Err(e) = validate_conditions(conditions) {
            warn!("Invalid conditions: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Validate actions if provided
    if let Some(ref actions) = input.actions {
        if let Err(e) = validate_actions(actions) {
            warn!("Invalid actions: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let update_input = CreatePolicyRule {
        tenant_id: existing.tenant_id,
        domain_id: existing.domain_id,
        name: input.name.unwrap_or(existing.name),
        description: input.description.or(existing.description),
        policy_type: input.policy_type.unwrap_or(existing.policy_type),
        priority: input.priority.unwrap_or(existing.priority),
        conditions: input.conditions.unwrap_or(existing.conditions),
        actions: input.actions.unwrap_or(existing.actions),
    };

    let policy = repo.update(policy_id, update_input).await.map_err(|e| {
        error!("Database error while updating policy: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Updated policy rule: {}", policy.name);

    Ok(Json(PolicyRuleResponse { rule: policy }))
}

/// Enable a policy rule
pub async fn enable_policy(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, policy_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = PolicyRepository::new(state.db_pool.clone());

    // Verify policy exists and belongs to tenant
    let policy = repo
        .get(policy_id)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if policy.tenant_id != Some(tenant_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    repo.enable(policy_id).await.map_err(|e| {
        error!("Database error while enabling policy: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Enabled policy: {}", policy.name);
    Ok(StatusCode::OK)
}

/// Disable a policy rule
pub async fn disable_policy(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, policy_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = PolicyRepository::new(state.db_pool.clone());

    // Verify policy exists and belongs to tenant
    let policy = repo
        .get(policy_id)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if policy.tenant_id != Some(tenant_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    repo.disable(policy_id).await.map_err(|e| {
        error!("Database error while disabling policy: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Disabled policy: {}", policy.name);
    Ok(StatusCode::OK)
}

/// Delete a policy rule
pub async fn delete_policy(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, policy_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let repo = PolicyRepository::new(state.db_pool.clone());

    // Verify policy exists and belongs to tenant
    let policy = repo
        .get(policy_id)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if policy.tenant_id != Some(tenant_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    repo.delete(policy_id).await.map_err(|e| {
        error!("Database error while deleting policy: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Deleted policy: {}", policy.name);
    Ok(StatusCode::NO_CONTENT)
}

/// Validate policy type
fn is_valid_policy_type(policy_type: &str) -> bool {
    matches!(
        policy_type.to_lowercase().as_str(),
        "inbound" | "outbound" | "both"
    )
}

/// Validate conditions format
fn validate_conditions(conditions: &serde_json::Value) -> Result<(), String> {
    if !conditions.is_array() {
        return Err("Conditions must be an array".to_string());
    }

    let arr = conditions.as_array().unwrap();
    for (i, condition) in arr.iter().enumerate() {
        if !condition.is_object() {
            return Err(format!("Condition {} must be an object", i));
        }

        let obj = condition.as_object().unwrap();
        if !obj.contains_key("condition_type") {
            return Err(format!("Condition {} missing condition_type", i));
        }
        if !obj.contains_key("operator") {
            return Err(format!("Condition {} missing operator", i));
        }
        if !obj.contains_key("value") {
            return Err(format!("Condition {} missing value", i));
        }
    }

    Ok(())
}

/// Validate actions format
fn validate_actions(actions: &serde_json::Value) -> Result<(), String> {
    if !actions.is_array() {
        return Err("Actions must be an array".to_string());
    }

    let arr = actions.as_array().unwrap();
    if arr.is_empty() {
        return Err("At least one action is required".to_string());
    }

    for (i, action) in arr.iter().enumerate() {
        if !action.is_object() {
            return Err(format!("Action {} must be an object", i));
        }

        let obj = action.as_object().unwrap();
        if !obj.contains_key("action_type") {
            return Err(format!("Action {} missing action_type", i));
        }
    }

    Ok(())
}
