//! Campaign handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use mairust_storage::models::{
    Campaign, CampaignStats, CampaignStatus, CreateCampaign, UpdateCampaign,
};
use mairust_storage::repository::CampaignRepository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

/// Error response
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

/// Query parameters for listing campaigns
#[derive(Debug, Deserialize)]
pub struct ListCampaignsQuery {
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// Campaign list response
#[derive(Debug, Serialize)]
pub struct CampaignListResponse {
    pub data: Vec<CampaignResponse>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Campaign response
#[derive(Debug, Serialize)]
pub struct CampaignResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub subject: String,
    pub from_address: String,
    pub from_name: Option<String>,
    pub status: String,
    pub total_recipients: i32,
    pub sent_count: i32,
    pub delivered_count: i32,
    pub bounced_count: i32,
    pub failed_count: i32,
    pub progress_percentage: f64,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Campaign> for CampaignResponse {
    fn from(c: Campaign) -> Self {
        let progress = c.progress_percentage();
        Self {
            id: c.id,
            name: c.name,
            description: c.description,
            subject: c.subject,
            from_address: c.from_address,
            from_name: c.from_name,
            status: c.status,
            total_recipients: c.total_recipients,
            sent_count: c.sent_count,
            delivered_count: c.delivered_count,
            bounced_count: c.bounced_count,
            failed_count: c.failed_count,
            progress_percentage: progress,
            scheduled_at: c.scheduled_at,
            started_at: c.started_at,
            completed_at: c.completed_at,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// Request body for creating a campaign
#[derive(Debug, Deserialize)]
pub struct CreateCampaignRequest {
    pub name: String,
    pub description: Option<String>,
    pub subject: String,
    pub from_address: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub recipient_list_id: Option<Uuid>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub rate_limit_per_hour: Option<i32>,
    pub rate_limit_per_minute: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

/// Request body for updating a campaign
#[derive(Debug, Deserialize)]
pub struct UpdateCampaignRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub recipient_list_id: Option<Uuid>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub rate_limit_per_hour: Option<i32>,
    pub rate_limit_per_minute: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

/// Request body for scheduling a campaign
#[derive(Debug, Deserialize)]
pub struct ScheduleCampaignRequest {
    pub scheduled_at: Option<DateTime<Utc>>,
}

/// List campaigns for a tenant
///
/// GET /api/v1/tenants/:tenant_id/campaigns
pub async fn list_campaigns(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<ListCampaignsQuery>,
) -> Result<Json<CampaignListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let repo = CampaignRepository::new(state.db_pool.pool().clone());

    let status = query.status.and_then(|s| s.parse::<CampaignStatus>().ok());

    let campaigns = repo
        .list_by_tenant(tenant_id, status, query.limit, query.offset)
        .await
        .map_err(|e| {
            error!("Failed to list campaigns: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to list campaigns".to_string(),
                }),
            )
        })?;

    let total = repo
        .count_by_tenant(tenant_id, status)
        .await
        .unwrap_or(0);

    let data = campaigns.into_iter().map(CampaignResponse::from).collect();

    Ok(Json(CampaignListResponse {
        data,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

/// Create a new campaign
///
/// POST /api/v1/tenants/:tenant_id/campaigns
pub async fn create_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreateCampaignRequest>,
) -> Result<(StatusCode, Json<CampaignResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    // Validate input
    if input.name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Campaign name is required".to_string(),
            }),
        ));
    }

    if input.subject.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Subject is required".to_string(),
            }),
        ));
    }

    if input.html_body.is_none() && input.text_body.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Either html_body or text_body is required".to_string(),
            }),
        ));
    }

    let repo = CampaignRepository::new(state.db_pool.pool().clone());

    let create_input = CreateCampaign {
        tenant_id,
        name: input.name,
        description: input.description,
        subject: input.subject,
        from_address: input.from_address,
        from_name: input.from_name,
        reply_to: input.reply_to,
        html_body: input.html_body,
        text_body: input.text_body,
        recipient_list_id: input.recipient_list_id,
        scheduled_at: input.scheduled_at,
        rate_limit_per_hour: input.rate_limit_per_hour,
        rate_limit_per_minute: input.rate_limit_per_minute,
        tags: input.tags,
        metadata: input.metadata,
    };

    let campaign = repo.create(create_input).await.map_err(|e| {
        error!("Failed to create campaign: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to create campaign".to_string(),
            }),
        )
    })?;

    info!("Created campaign {} for tenant {}", campaign.id, tenant_id);

    Ok((StatusCode::CREATED, Json(CampaignResponse::from(campaign))))
}

/// Get a campaign by ID
///
/// GET /api/v1/tenants/:tenant_id/campaigns/:campaign_id
pub async fn get_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CampaignResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let repo = CampaignRepository::new(state.db_pool.pool().clone());

    let campaign = repo
        .get_by_tenant(tenant_id, campaign_id)
        .await
        .map_err(|e| {
            error!("Failed to get campaign: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to get campaign".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Campaign not found".to_string(),
                }),
            )
        })?;

    Ok(Json(CampaignResponse::from(campaign)))
}

/// Update a campaign
///
/// PUT /api/v1/tenants/:tenant_id/campaigns/:campaign_id
pub async fn update_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateCampaignRequest>,
) -> Result<Json<CampaignResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let repo = CampaignRepository::new(state.db_pool.pool().clone());

    let update_input = UpdateCampaign {
        name: input.name,
        description: input.description,
        subject: input.subject,
        from_address: input.from_address,
        from_name: input.from_name,
        reply_to: input.reply_to,
        html_body: input.html_body,
        text_body: input.text_body,
        recipient_list_id: input.recipient_list_id,
        scheduled_at: input.scheduled_at,
        rate_limit_per_hour: input.rate_limit_per_hour,
        rate_limit_per_minute: input.rate_limit_per_minute,
        tags: input.tags,
        metadata: input.metadata,
    };

    let campaign = repo
        .update(campaign_id, tenant_id, update_input)
        .await
        .map_err(|e| {
            error!("Failed to update campaign: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to update campaign".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Campaign not found or not in draft status".to_string(),
                }),
            )
        })?;

    info!("Updated campaign {}", campaign_id);

    Ok(Json(CampaignResponse::from(campaign)))
}

/// Delete a campaign
///
/// DELETE /api/v1/tenants/:tenant_id/campaigns/:campaign_id
pub async fn delete_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let repo = CampaignRepository::new(state.db_pool.pool().clone());

    let deleted = repo.delete(campaign_id, tenant_id).await.map_err(|e| {
        error!("Failed to delete campaign: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to delete campaign".to_string(),
            }),
        )
    })?;

    if deleted {
        info!("Deleted campaign {}", campaign_id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "Campaign not found or not in draft status".to_string(),
            }),
        ))
    }
}

/// Schedule a campaign for sending
///
/// POST /api/v1/tenants/:tenant_id/campaigns/:campaign_id/schedule
pub async fn schedule_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<ScheduleCampaignRequest>,
) -> Result<Json<CampaignResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    // Create campaign manager
    let campaign_manager = mairust_core::CampaignManager::new(
        state.db_pool.clone(),
        "https://mail.example.com/unsubscribe".to_string(), // TODO: Get from config
    );

    let campaign = campaign_manager
        .schedule_campaign(tenant_id, campaign_id, input.scheduled_at)
        .await
        .map_err(|e| {
            error!("Failed to schedule campaign: {}", e);
            let (status, message) = match e {
                mairust_core::CampaignError::NotFound => {
                    (StatusCode::NOT_FOUND, "Campaign not found")
                }
                mairust_core::CampaignError::NotDraft => {
                    (StatusCode::BAD_REQUEST, "Campaign is not in draft status")
                }
                mairust_core::CampaignError::NoRecipientList => {
                    (StatusCode::BAD_REQUEST, "Campaign has no recipient list")
                }
                mairust_core::CampaignError::EmptyRecipientList => {
                    (StatusCode::BAD_REQUEST, "Recipient list is empty")
                }
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to schedule campaign"),
            };
            (
                status,
                Json(ErrorResponse {
                    error: "schedule_error".to_string(),
                    message: message.to_string(),
                }),
            )
        })?;

    info!("Scheduled campaign {} for tenant {}", campaign_id, tenant_id);

    Ok(Json(CampaignResponse::from(campaign)))
}

/// Start sending a campaign immediately
///
/// POST /api/v1/tenants/:tenant_id/campaigns/:campaign_id/send
pub async fn send_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CampaignResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let campaign_manager = mairust_core::CampaignManager::new(
        state.db_pool.clone(),
        "https://mail.example.com/unsubscribe".to_string(),
    );

    let campaign = campaign_manager
        .start_campaign(tenant_id, campaign_id)
        .await
        .map_err(|e| {
            error!("Failed to start campaign: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "send_error".to_string(),
                    message: e.to_string(),
                }),
            )
        })?;

    info!("Started campaign {} for tenant {}", campaign_id, tenant_id);

    Ok(Json(CampaignResponse::from(campaign)))
}

/// Pause a sending campaign
///
/// POST /api/v1/tenants/:tenant_id/campaigns/:campaign_id/pause
pub async fn pause_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CampaignResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let campaign_manager = mairust_core::CampaignManager::new(
        state.db_pool.clone(),
        "https://mail.example.com/unsubscribe".to_string(),
    );

    let campaign = campaign_manager
        .pause_campaign(tenant_id, campaign_id)
        .await
        .map_err(|e| {
            error!("Failed to pause campaign: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "pause_error".to_string(),
                    message: e.to_string(),
                }),
            )
        })?;

    info!("Paused campaign {}", campaign_id);

    Ok(Json(CampaignResponse::from(campaign)))
}

/// Resume a paused campaign
///
/// POST /api/v1/tenants/:tenant_id/campaigns/:campaign_id/resume
pub async fn resume_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CampaignResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let campaign_manager = mairust_core::CampaignManager::new(
        state.db_pool.clone(),
        "https://mail.example.com/unsubscribe".to_string(),
    );

    let campaign = campaign_manager
        .resume_campaign(tenant_id, campaign_id)
        .await
        .map_err(|e| {
            error!("Failed to resume campaign: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "resume_error".to_string(),
                    message: e.to_string(),
                }),
            )
        })?;

    info!("Resumed campaign {}", campaign_id);

    Ok(Json(CampaignResponse::from(campaign)))
}

/// Cancel a campaign
///
/// POST /api/v1/tenants/:tenant_id/campaigns/:campaign_id/cancel
pub async fn cancel_campaign(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CampaignResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let campaign_manager = mairust_core::CampaignManager::new(
        state.db_pool.clone(),
        "https://mail.example.com/unsubscribe".to_string(),
    );

    let campaign = campaign_manager
        .cancel_campaign(tenant_id, campaign_id)
        .await
        .map_err(|e| {
            error!("Failed to cancel campaign: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "cancel_error".to_string(),
                    message: e.to_string(),
                }),
            )
        })?;

    info!("Cancelled campaign {}", campaign_id);

    Ok(Json(CampaignResponse::from(campaign)))
}

/// Get campaign statistics
///
/// GET /api/v1/tenants/:tenant_id/campaigns/:campaign_id/stats
pub async fn get_campaign_stats(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, campaign_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CampaignStats>, (StatusCode, Json<ErrorResponse>)> {
    require_tenant_access(&auth, tenant_id).map_err(|status| {
        (
            status,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Not authorized for this tenant".to_string(),
            }),
        )
    })?;

    let campaign_manager = mairust_core::CampaignManager::new(
        state.db_pool.clone(),
        "https://mail.example.com/unsubscribe".to_string(),
    );

    let stats = campaign_manager
        .get_campaign_stats(tenant_id, campaign_id)
        .await
        .map_err(|e| {
            error!("Failed to get campaign stats: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to get campaign statistics".to_string(),
                }),
            )
        })?;

    Ok(Json(stats))
}
