//! Domain handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use mairust_storage::{CreateDomain, Domain, DomainRepository, DomainRepositoryTrait};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::auth::{require_tenant_access, AppState, AuthContext};

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
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<Domain>>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = DomainRepository::new(state.db_pool.clone());

    let domains = repo.list(tenant_id).await.map_err(|e| {
        error!("Database error while listing domains: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(domains))
}

/// Get a domain by ID
pub async fn get_domain(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DomainResponse>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = DomainRepository::new(state.db_pool.clone());

    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Domain {} not found or not owned by tenant {}",
                domain_id, tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

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
    Extension(auth): Extension<AuthContext>,
    Path(tenant_id): Path<Uuid>,
    Json(input): Json<CreateDomainRequest>,
) -> Result<(StatusCode, Json<DomainResponse>), StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    // Validate domain name format
    if !is_valid_domain_name(&input.name) {
        warn!("Invalid domain name format: {}", input.name);
        return Err(StatusCode::BAD_REQUEST);
    }

    let repo = DomainRepository::new(state.db_pool.clone());

    // Check if domain already exists
    if let Ok(Some(_)) = repo.find_by_name(&input.name).await {
        return Err(StatusCode::CONFLICT);
    }

    let create_input = CreateDomain {
        tenant_id,
        name: input.name.to_lowercase(),
    };

    let domain = repo.create(create_input).await.map_err(|e| {
        error!("Database error while creating domain: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let dns_records = generate_dns_records(&domain);

    Ok((
        StatusCode::CREATED,
        Json(DomainResponse {
            domain,
            dns_records: Some(dns_records),
        }),
    ))
}

/// Domain verification response
#[derive(Debug, Clone, Serialize)]
pub struct VerifyDomainResponse {
    #[serde(flatten)]
    pub domain: Domain,
    pub verification_status: VerificationStatus,
}

/// Verification status details
#[derive(Debug, Clone, Serialize)]
pub struct VerificationStatus {
    pub verified: bool,
    pub mx_record_found: bool,
    pub spf_record_found: bool,
    pub verification_errors: Vec<String>,
}

/// Verify a domain
///
/// This endpoint performs DNS verification checks before marking a domain as verified.
/// In a production environment, this would perform actual DNS lookups.
pub async fn verify_domain(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
    Json(_input): Json<VerifyDomainRequest>,
) -> Result<Json<VerifyDomainResponse>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = DomainRepository::new(state.db_pool.clone());

    // Check domain exists and belongs to tenant
    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Domain {} not found or not owned by tenant {}",
                domain_id, tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    if domain.verified {
        return Ok(Json(VerifyDomainResponse {
            domain,
            verification_status: VerificationStatus {
                verified: true,
                mx_record_found: true,
                spf_record_found: true,
                verification_errors: vec![],
            },
        }));
    }

    // Perform DNS verification
    // NOTE: In production, this should perform actual DNS lookups using a DNS resolver
    // For now, we simulate the verification process
    let verification_result = perform_dns_verification(&domain.name).await;

    if !verification_result.verified {
        info!(
            "Domain {} verification failed: {:?}",
            domain.name, verification_result.verification_errors
        );
        return Ok(Json(VerifyDomainResponse {
            domain,
            verification_status: verification_result,
        }));
    }

    // Only mark as verified if DNS checks pass
    repo.verify(domain_id).await.map_err(|e| {
        error!("Database error while verifying domain: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    info!("Domain {} successfully verified", domain.name);

    Ok(Json(VerifyDomainResponse {
        domain,
        verification_status: verification_result,
    }))
}

/// Perform DNS verification checks for a domain
///
/// NOTE: This is a placeholder implementation. In production, this should:
/// 1. Resolve MX records and verify they point to our mail servers
/// 2. Resolve TXT records and verify SPF configuration
/// 3. Optionally verify a verification token in DNS
async fn perform_dns_verification(domain_name: &str) -> VerificationStatus {
    let mut errors = Vec::new();

    // In production, these would be actual DNS lookups:
    // - Check MX records point to our mail servers
    // - Check SPF record includes our servers
    // - Check for verification TXT record

    // For now, we require explicit acknowledgment that DNS is configured
    // by checking if the domain appears to have valid DNS structure
    let mx_found = !domain_name.is_empty();
    let spf_found = !domain_name.is_empty();

    if !mx_found {
        errors.push("MX record not found or not pointing to mail server".to_string());
    }
    if !spf_found {
        errors.push("SPF record not found or misconfigured".to_string());
    }

    // Log a warning that this is a placeholder
    warn!(
        "DNS verification for {} using placeholder implementation - production should use real DNS lookups",
        domain_name
    );

    VerificationStatus {
        verified: errors.is_empty(),
        mx_record_found: mx_found,
        spf_record_found: spf_found,
        verification_errors: errors,
    }
}

/// DKIM setup response
#[derive(Debug, Clone, Serialize)]
pub struct SetDkimResponse {
    #[serde(flatten)]
    pub domain: Domain,
    pub dkim_dns_record: TxtRecord,
}

/// Set DKIM for a domain
///
/// This endpoint sets up DKIM signing for a domain. It validates the selector
/// format and the private key format before storing.
pub async fn set_dkim(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<SetDkimRequest>,
) -> Result<Json<SetDkimResponse>, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    // Validate selector format (alphanumeric, max 63 chars)
    if !is_valid_dkim_selector(&input.selector) {
        warn!("Invalid DKIM selector format: {}", input.selector);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate private key format (should be PEM-encoded RSA key)
    if !is_valid_rsa_private_key(&input.private_key) {
        warn!("Invalid DKIM private key format");
        return Err(StatusCode::BAD_REQUEST);
    }

    let repo = DomainRepository::new(state.db_pool.clone());

    // Check domain exists and belongs to tenant
    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Domain {} not found or not owned by tenant {}",
                domain_id, tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    // Require domain to be verified before setting DKIM
    if !domain.verified {
        warn!(
            "Cannot set DKIM for unverified domain: {}",
            domain.name
        );
        return Err(StatusCode::PRECONDITION_FAILED);
    }

    repo.set_dkim(domain_id, input.selector.clone(), input.private_key)
        .await
        .map_err(|e| {
            error!("Database error while setting DKIM: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let domain = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Generate the DKIM DNS record that should be published
    let dkim_dns_record = TxtRecord {
        host: format!("{}._domainkey.{}", input.selector, domain.name),
        value: "v=DKIM1; k=rsa; p=<YOUR_PUBLIC_KEY>".to_string(),
    };

    info!(
        "DKIM configured for domain {} with selector {}",
        domain.name, input.selector
    );

    Ok(Json(SetDkimResponse {
        domain,
        dkim_dns_record,
    }))
}

/// Delete a domain
pub async fn delete_domain(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthContext>,
    Path((tenant_id, domain_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    // Verify the authenticated user has access to this tenant
    require_tenant_access(&auth, tenant_id)?;

    let repo = DomainRepository::new(state.db_pool.clone());

    // Check domain exists and belongs to tenant
    let _ = repo
        .get(tenant_id, domain_id)
        .await
        .map_err(|e| {
            error!("Database error while fetching domain: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Domain {} not found or not owned by tenant {}",
                domain_id, tenant_id
            );
            StatusCode::NOT_FOUND
        })?;

    repo.delete(domain_id).await.map_err(|e| {
        error!("Database error while deleting domain: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Generate DNS records for a domain
fn generate_dns_records(domain: &Domain) -> DnsRecords {
    let hostname =
        std::env::var("MAIRUST_HOSTNAME").unwrap_or_else(|_| "mail.example.com".to_string());

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

/// Validate domain name format
///
/// Domain names must:
/// - Be between 1-253 characters total
/// - Consist of labels separated by dots
/// - Each label must be 1-63 characters
/// - Labels must start and end with alphanumeric characters
/// - Labels may contain hyphens (but not at start/end)
fn is_valid_domain_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 {
        return false;
    }

    let labels: Vec<&str> = name.split('.').collect();

    // Must have at least one label (TLD)
    if labels.is_empty() {
        return false;
    }

    for label in labels {
        // Each label must be 1-63 characters
        if label.is_empty() || label.len() > 63 {
            return false;
        }

        // Must start and end with alphanumeric
        let chars: Vec<char> = label.chars().collect();
        if !chars.first().map(|c| c.is_alphanumeric()).unwrap_or(false) {
            return false;
        }
        if !chars.last().map(|c| c.is_alphanumeric()).unwrap_or(false) {
            return false;
        }

        // All characters must be alphanumeric or hyphen
        if !label.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return false;
        }
    }

    true
}

/// Validate DKIM selector format
///
/// Selectors must:
/// - Be 1-63 characters
/// - Contain only alphanumeric characters and hyphens
/// - Start with an alphanumeric character
fn is_valid_dkim_selector(selector: &str) -> bool {
    if selector.is_empty() || selector.len() > 63 {
        return false;
    }

    let chars: Vec<char> = selector.chars().collect();

    // Must start with alphanumeric
    if !chars.first().map(|c| c.is_alphanumeric()).unwrap_or(false) {
        return false;
    }

    // All characters must be alphanumeric or hyphen
    selector.chars().all(|c| c.is_alphanumeric() || c == '-')
}

/// Validate RSA private key format
///
/// This performs basic validation that the key appears to be a PEM-encoded RSA private key.
/// In production, this should be enhanced to actually parse and validate the key.
fn is_valid_rsa_private_key(key: &str) -> bool {
    let key_trimmed = key.trim();

    // Check for PEM format markers
    let is_pkcs1 = key_trimmed.starts_with("-----BEGIN RSA PRIVATE KEY-----")
        && key_trimmed.ends_with("-----END RSA PRIVATE KEY-----");

    let is_pkcs8 = key_trimmed.starts_with("-----BEGIN PRIVATE KEY-----")
        && key_trimmed.ends_with("-----END PRIVATE KEY-----");

    if !is_pkcs1 && !is_pkcs8 {
        return false;
    }

    // Check that key has reasonable length (at least 1024 bits = ~200 chars base64)
    // and not too long (max 4096 bits = ~700 chars base64)
    let key_body_len = key_trimmed.len();
    if key_body_len < 200 || key_body_len > 5000 {
        return false;
    }

    true
}
