//! Domain handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use mairust_storage::{CreateDomain, Domain, DomainRepository, DomainRepositoryTrait};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AppState;

/// Request body for creating a domain
#[derive(Debug, Clone, Deserialize)]
pub struct CreateDomainRequest {
    pub name: String,
}

/// Request body for verifying a domain
#[derive(Debug, Clone, Deserialize)]
pub struct VerifyDomainRequest {
    pub verification_token: Option<String>,
}

/// Request body for setting DKIM
#[derive(Debug, Clone, Deserialize)]
pub struct SetDkimRequest {
    pub selector: String,
    pub private_key: String,
}

/// Response for domain with verification info
#[derive(Debug, Clone, Serialize)]
pub struct DomainResponse {
    #[serde(flatten)]
    pub domain: Domain,
    pub dns_records: Option<DnsRecords>,
}

/// DNS records needed for domain verification
#[derive(Debug, Clone, Serialize)]
pub struct DnsRecords {
    pub mx: MxRecord,
    pub spf: TxtRecord,
    pub dkim: Option<TxtRecord>,
    pub dmarc: TxtRecord,
}

#[derive(Debug, Clone, Serialize)]
pub struct MxRecord {
    pub host: String,
    pub priority: u16,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TxtRecord {
    pub host: String,
    pub value: String,
}

/// List domains for a tenant
pub async fn list_domains(
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<Domain>>, StatusCode> {
    let repo = DomainRepository::new(state.db_pool.clone());

    let domains = repo
        .list(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(domains))
}

/// Get a domain by ID
pub async fn get_domain(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DomainResponse>, StatusCode> {
    let repo = DomainRepository::new(state.db_pool.clone());

    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Generate DNS records info
    let dns_records = generate_dns_records(&domain);

    Ok(Json(DomainResponse {
        domain,
        dns_records: Some(dns_records),
    }))
}

/// Create a new domain
pub async fn create_domain(
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreateDomainRequest>,
) -> Result<(StatusCode, Json<DomainResponse>), StatusCode> {
    let repo = DomainRepository::new(state.db_pool.clone());

    // Check if domain already exists
    if let Ok(Some(_)) = repo.find_by_name(&input.name).await {
        return Err(StatusCode::CONFLICT);
    }

    let create_input = CreateDomain {
        tenant_id,
        name: input.name.to_lowercase(),
    };

    let domain = repo
        .create(create_input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let dns_records = generate_dns_records(&domain);

    Ok((
        StatusCode::CREATED,
        Json(DomainResponse {
            domain,
            dns_records: Some(dns_records),
        }),
    ))
}

/// Verify a domain
pub async fn verify_domain(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
    Json(_input): Json<VerifyDomainRequest>,
) -> Result<Json<Domain>, StatusCode> {
    let repo = DomainRepository::new(state.db_pool.clone());

    // Check domain exists and belongs to tenant
    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if domain.verified {
        return Ok(Json(domain));
    }

    // TODO: Implement actual DNS verification
    // For now, just mark as verified
    repo.verify(domain_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(domain))
}

/// Set DKIM for a domain
pub async fn set_dkim(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<SetDkimRequest>,
) -> Result<Json<Domain>, StatusCode> {
    let repo = DomainRepository::new(state.db_pool.clone());

    // Check domain exists and belongs to tenant
    let _ = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.set_dkim(domain_id, input.selector, input.private_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(domain))
}

/// Delete a domain
pub async fn delete_domain(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let repo = DomainRepository::new(state.db_pool.clone());

    // Check domain exists and belongs to tenant
    let _ = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo.delete(domain_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Generate DNS records for a domain
fn generate_dns_records(domain: &Domain) -> DnsRecords {
    let hostname = std::env::var("MAIRUST_HOSTNAME").unwrap_or_else(|_| "mail.example.com".to_string());

    DnsRecords {
        mx: MxRecord {
            host: domain.name.clone(),
            priority: 10,
            value: hostname.clone(),
        },
        spf: TxtRecord {
            host: domain.name.clone(),
            value: format!("v=spf1 mx a:{} ~all", hostname),
        },
        dkim: domain.dkim_selector.as_ref().map(|selector| TxtRecord {
            host: format!("{}._domainkey.{}", selector, domain.name),
            value: "v=DKIM1; k=rsa; p=<public_key>".to_string(),
        }),
        dmarc: TxtRecord {
            host: format!("_dmarc.{}", domain.name),
            value: format!("v=DMARC1; p=none; rua=mailto:dmarc@{}", domain.name),
        },
    }
}
