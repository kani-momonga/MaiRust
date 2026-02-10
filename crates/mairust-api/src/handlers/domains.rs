//! Domain handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use mairust_storage::{CreateDomain, Domain, DomainRepository, DomainRepositoryTrait};
use rsa::pkcs8::{DecodePrivateKey, EncodePublicKey};
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::TokioAsyncResolver;
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

    // Check if domain already exists (global check to prevent duplicates across tenants)
    if let Ok(Some(_)) = repo.find_by_name(&input.name).await {
        return Err(StatusCode::CONFLICT);
    }

    // Also check within tenant for safety
    if let Ok(Some(_)) = repo.find_by_name_for_tenant(tenant_id, &input.name).await {
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
/// This function performs actual DNS lookups to verify:
/// 1. MX records exist and point to a mail server
/// 2. SPF TXT record exists and is properly configured
///
/// Both checks must pass for the domain to be marked as verified.
async fn perform_dns_verification(domain_name: &str) -> VerificationStatus {
    let mut errors = Vec::new();

    // Create DNS resolver with default system configuration
    let resolver =
        TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

    // Get expected hostname for MX verification
    let expected_hostname =
        std::env::var("MAIRUST_HOSTNAME").unwrap_or_else(|_| "mail.example.com".to_string());

    // Check MX records
    let mx_found = match resolver.mx_lookup(domain_name).await {
        Ok(mx_response) => {
            let mx_records: Vec<_> = mx_response.iter().collect();
            if mx_records.is_empty() {
                errors.push("No MX records found for domain".to_string());
                false
            } else {
                // Check if any MX record points to our mail server
                let has_valid_mx = mx_records.iter().any(|mx| {
                    let exchange = mx.exchange().to_string();
                    let exchange_trimmed = exchange.trim_end_matches('.');
                    debug!(
                        "Found MX record: {} (priority: {})",
                        exchange_trimmed,
                        mx.preference()
                    );
                    exchange_trimmed.eq_ignore_ascii_case(&expected_hostname)
                        || exchange_trimmed.ends_with(&format!(".{}", expected_hostname))
                });

                if has_valid_mx {
                    info!(
                        "MX record verified for domain {} pointing to {}",
                        domain_name, expected_hostname
                    );
                    true
                } else {
                    let found_records: Vec<String> = mx_records
                        .iter()
                        .map(|mx| mx.exchange().to_string().trim_end_matches('.').to_string())
                        .collect();
                    errors.push(format!(
                        "MX records found ({}) but none point to {}",
                        found_records.join(", "),
                        expected_hostname
                    ));
                    false
                }
            }
        }
        Err(e) => {
            warn!("MX lookup failed for {}: {}", domain_name, e);
            errors.push(format!("MX record lookup failed: {}", e));
            false
        }
    };

    // Check SPF TXT record
    let spf_found = match resolver.txt_lookup(domain_name).await {
        Ok(txt_response) => {
            let txt_records: Vec<String> = txt_response
                .iter()
                .map(|txt| {
                    txt.iter()
                        .map(|data| String::from_utf8_lossy(data).to_string())
                        .collect::<Vec<_>>()
                        .join("")
                })
                .collect();

            debug!("Found TXT records for {}: {:?}", domain_name, txt_records);

            // Look for SPF record
            let spf_record = txt_records.iter().find(|r| r.starts_with("v=spf1"));

            match spf_record {
                Some(spf) => {
                    // Verify SPF record includes our mail server
                    // Accept records that include 'mx', 'a:', 'include:', or the expected hostname
                    let spf_lower = spf.to_lowercase();
                    let is_valid_spf = spf_lower.contains("mx")
                        || spf_lower.contains(&format!("a:{}", expected_hostname.to_lowercase()))
                        || spf_lower.contains(&format!(
                            "include:{}",
                            expected_hostname.to_lowercase()
                        ))
                        || spf_lower.contains(&expected_hostname.to_lowercase());

                    if is_valid_spf {
                        info!("SPF record verified for domain {}: {}", domain_name, spf);
                        true
                    } else {
                        errors.push(format!(
                            "SPF record found but does not include mail server: {}",
                            spf
                        ));
                        false
                    }
                }
                None => {
                    errors.push("No SPF record (v=spf1) found in TXT records".to_string());
                    false
                }
            }
        }
        Err(e) => {
            warn!("TXT lookup failed for {}: {}", domain_name, e);
            errors.push(format!("TXT record lookup failed: {}", e));
            false
        }
    };

    let verified = mx_found && spf_found;

    if verified {
        info!(
            "Domain {} successfully verified (MX and SPF records confirmed)",
            domain_name
        );
    } else {
        warn!(
            "Domain {} verification failed: {:?}",
            domain_name, errors
        );
    }

    VerificationStatus {
        verified,
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
/// format and the private key format before storing. The public key is extracted
/// from the private key and returned in the DNS record format.
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

    // Parse and validate the private key, extracting the public key
    let public_key_base64 = match extract_public_key_from_pem(&input.private_key) {
        Ok(pk) => pk,
        Err(e) => {
            warn!("Invalid DKIM private key: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

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

    // Generate the DKIM DNS record with the actual public key
    let dkim_dns_record = TxtRecord {
        host: format!("{}._domainkey.{}", input.selector, domain.name),
        value: format!("v=DKIM1; k=rsa; p={}", public_key_base64),
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

    // Extract public key from DKIM private key if configured
    let dkim_record = match (&domain.dkim_selector, &domain.dkim_private_key) {
        (Some(selector), Some(private_key)) => {
            match extract_public_key_from_pem(private_key) {
                Ok(public_key_base64) => Some(TxtRecord {
                    host: format!("{}._domainkey.{}", selector, domain.name),
                    value: format!("v=DKIM1; k=rsa; p={}", public_key_base64),
                }),
                Err(e) => {
                    warn!(
                        "Failed to extract public key for domain {}: {}",
                        domain.name, e
                    );
                    None
                }
            }
        }
        (Some(selector), None) => {
            // Selector is set but no private key - indicate DKIM is not fully configured
            warn!(
                "Domain {} has DKIM selector {} but no private key configured",
                domain.name, selector
            );
            None
        }
        _ => None,
    };

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
        dkim: dkim_record,
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

/// Extract public key from a PEM-encoded RSA private key
///
/// This function parses the private key, validates it, and extracts the public key
/// in the format needed for DKIM DNS records (DER-encoded, base64).
///
/// Returns the base64-encoded public key suitable for the `p=` parameter in DKIM records.
fn extract_public_key_from_pem(pem_key: &str) -> Result<String, String> {
    let key_trimmed = pem_key.trim();

    // Try to parse as PKCS#8 first (-----BEGIN PRIVATE KEY-----)
    let private_key = if key_trimmed.starts_with("-----BEGIN PRIVATE KEY-----") {
        RsaPrivateKey::from_pkcs8_pem(key_trimmed)
            .map_err(|e| format!("Failed to parse PKCS#8 private key: {}", e))?
    } else if key_trimmed.starts_with("-----BEGIN RSA PRIVATE KEY-----") {
        // PKCS#1 format - need to use different parser
        use rsa::pkcs1::DecodeRsaPrivateKey;
        RsaPrivateKey::from_pkcs1_pem(key_trimmed)
            .map_err(|e| format!("Failed to parse PKCS#1 private key: {}", e))?
    } else {
        return Err("Invalid PEM format: expected PKCS#1 or PKCS#8 private key".to_string());
    };

    // Validate key size (DKIM requires at least 1024 bits, recommend 2048+)
    let key_bits = private_key.size() * 8;
    if key_bits < 1024 {
        return Err(format!(
            "Key size {} bits is too small for DKIM (minimum 1024 bits)",
            key_bits
        ));
    }
    if key_bits > 4096 {
        return Err(format!(
            "Key size {} bits exceeds maximum supported (4096 bits)",
            key_bits
        ));
    }

    // Extract the public key and encode as DER
    let public_key = private_key.to_public_key();
    let public_key_der = public_key
        .to_public_key_der()
        .map_err(|e| format!("Failed to encode public key: {}", e))?;

    // Base64 encode for DKIM DNS record
    Ok(BASE64_STANDARD.encode(public_key_der.as_bytes()))
}
