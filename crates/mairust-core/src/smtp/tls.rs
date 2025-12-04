//! TLS support for SMTP

use anyhow::{anyhow, Result};
use mairust_common::config::TlsConfig;
use rustls::pki_types::CertificateDer;
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tracing::info;

/// Load TLS configuration and create an acceptor
pub fn create_tls_acceptor(tls_config: &TlsConfig) -> Result<TlsAcceptor> {
    // Load certificates
    let cert_file = File::open(&tls_config.cert_path)
        .map_err(|e| anyhow!("Failed to open certificate file: {}", e))?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<CertificateDer<'static>> = certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to parse certificates: {}", e))?;

    if certs.is_empty() {
        return Err(anyhow!("No certificates found in certificate file"));
    }

    info!("Loaded {} certificate(s)", certs.len());

    // Load private key
    let key_file = File::open(&tls_config.key_path)
        .map_err(|e| anyhow!("Failed to open key file: {}", e))?;
    let mut key_reader = BufReader::new(key_file);
    let key = private_key(&mut key_reader)
        .map_err(|e| anyhow!("Failed to read private key: {}", e))?
        .ok_or_else(|| anyhow!("No private key found in key file"))?;

    // Create TLS config
    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| anyhow!("Failed to create TLS config: {}", e))?;

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

/// Check if TLS is properly configured
pub fn is_tls_configured(tls_config: &Option<TlsConfig>) -> bool {
    if let Some(config) = tls_config {
        config.cert_path.exists() && config.key_path.exists()
    } else {
        false
    }
}
