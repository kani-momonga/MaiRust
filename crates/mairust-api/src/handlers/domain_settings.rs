//! Domain settings handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use mairust_storage::{
    DomainRepository, DomainRepositoryTrait, DomainSettings, DomainSettingsRepository,
    DomainSettingsRepositoryTrait, UpdateDomainSettings,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// Request body for updating domain settings
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateDomainSettingsRequest {
    pub catch_all_enabled: Option<bool>,
    pub catch_all_mailbox_id: Option<Uuid>,
    pub max_message_size: Option<i64>,
    pub max_recipients: Option<i32>,
    pub rate_limit_per_hour: Option<i32>,
    pub require_tls_inbound: Option<bool>,
    pub require_tls_outbound: Option<bool>,
    pub spf_policy: Option<String>,
    pub dmarc_policy: Option<String>,
    pub extra_settings: Option<serde_json::Value>,
}

/// Response for domain settings
#[derive(Debug, Clone, Serialize)]
pub struct DomainSettingsResponse {
    #[serde(flatten)]
    pub settings: DomainSettings,
    pub domain_name: String,
}

/// Get domain settings
pub async fn get_domain_settings(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DomainSettingsResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let domain_repo = DomainRepository::new(state.db_pool.clone());
    let settings_repo = DomainSettingsRepository::new(state.db_pool.clone());

    // Verify domain exists and belongs to tenant
    let domain = domain_repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!("Domain {} not found for tenant {}", domain_id, tenant_id);
            StatusCode::NOT_FOUND
        })?;

    let settings = settings_repo.get_or_create(domain_id).await.map_err(|e| {
        error!("Database error while fetching domain settings: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(DomainSettingsResponse {
        settings,
        domain_name: domain.name,
    }))
}

/// Update domain settings
pub async fn update_domain_settings(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateDomainSettingsRequest>,
) -> Result<Json<DomainSettingsResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let domain_repo = DomainRepository::new(state.db_pool.clone());
    let settings_repo = DomainSettingsRepository::new(state.db_pool.clone());

    // Verify domain exists and belongs to tenant
    let domain = domain_repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!("Domain {} not found for tenant {}", domain_id, tenant_id);
            StatusCode::NOT_FOUND
        })?;

    // Validate SPF policy if provided
    if let Some(ref policy) = input.spf_policy {
        if !is_valid_spf_policy(policy) {
            warn!("Invalid SPF policy: {}", policy);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Validate DMARC policy if provided
    if let Some(ref policy) = input.dmarc_policy {
        if !is_valid_dmarc_policy(policy) {
            warn!("Invalid DMARC policy: {}", policy);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let update_input = UpdateDomainSettings {
        catch_all_enabled: input.catch_all_enabled,
        catch_all_mailbox_id: input.catch_all_mailbox_id,
        max_message_size: input.max_message_size,
        max_recipients: input.max_recipients,
        rate_limit_per_hour: input.rate_limit_per_hour,
        require_tls_inbound: input.require_tls_inbound,
        require_tls_outbound: input.require_tls_outbound,
        spf_policy: input.spf_policy,
        dmarc_policy: input.dmarc_policy,
        extra_settings: input.extra_settings,
    };

    let settings = settings_repo
        .update(domain_id, update_input)
        .await
        .map_err(|e| {
            error!("Database error while updating domain settings: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!("Updated settings for domain {}", domain.name);

    Ok(Json(DomainSettingsResponse {
        settings,
        domain_name: domain.name,
    }))
}

/// Enable catch-all for a domain
pub async fn enable_catch_all(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<CatchAllRequest>,
) -> Result<Json<DomainSettingsResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let domain_repo = DomainRepository::new(state.db_pool.clone());
    let settings_repo = DomainSettingsRepository::new(state.db_pool.clone());

    // Verify domain exists
    let domain = domain_repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let update = UpdateDomainSettings {
        catch_all_enabled: Some(true),
        catch_all_mailbox_id: Some(input.mailbox_id),
        max_message_size: None,
        max_recipients: None,
        rate_limit_per_hour: None,
        require_tls_inbound: None,
        require_tls_outbound: None,
        spf_policy: None,
        dmarc_policy: None,
        extra_settings: None,
    };

    let settings = settings_repo.update(domain_id, update).await.map_err(|e| {
        error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Enabled catch-all for domain {}", domain.name);

    Ok(Json(DomainSettingsResponse {
        settings,
        domain_name: domain.name,
    }))
}

/// Disable catch-all for a domain
pub async fn disable_catch_all(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DomainSettingsResponse>, StatusCode> {
    require_tenant_access(&auth, tenant_id)?;

    let domain_repo = DomainRepository::new(state.db_pool.clone());
    let settings_repo = DomainSettingsRepository::new(state.db_pool.clone());

    // Verify domain exists
    let domain = domain_repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let update = UpdateDomainSettings {
        catch_all_enabled: Some(false),
        catch_all_mailbox_id: None,
        max_message_size: None,
        max_recipients: None,
        rate_limit_per_hour: None,
        require_tls_inbound: None,
        require_tls_outbound: None,
        spf_policy: None,
        dmarc_policy: None,
        extra_settings: None,
    };

    let settings = settings_repo.update(domain_id, update).await.map_err(|e| {
        error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Disabled catch-all for domain {}", domain.name);

    Ok(Json(DomainSettingsResponse {
        settings,
        domain_name: domain.name,
    }))
}

/// Catch-all request body
#[derive(Debug, Clone, Deserialize)]
pub struct CatchAllRequest {
    pub mailbox_id: Uuid,
}

/// Validate SPF policy values
fn is_valid_spf_policy(policy: &str) -> bool {
    matches!(
        policy.to_lowercase().as_str(),
        "neutral" | "softfail" | "fail" | "pass"
    )
}

/// Validate DMARC policy values
fn is_valid_dmarc_policy(policy: &str) -> bool {
    matches!(
        policy.to_lowercase().as_str(),
        "none" | "quarantine" | "reject"
    )
}
