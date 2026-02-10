//! SMTP Authentication module

use anyhow::{anyhow, Result};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use mairust_storage::db::DatabasePool;
use mairust_storage::models::User;
use mairust_storage::repository::users::{DbUserRepository, UserRepository};
use tracing::{debug, warn};

/// SMTP Authentication result
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub success: bool,
    pub user: Option<User>,
    pub error: Option<String>,
}

impl AuthResult {
    pub fn success(user: User) -> Self {
        Self {
            success: true,
            user: Some(user),
            error: None,
        }
    }

    pub fn failure(error: &str) -> Self {
        Self {
            success: false,
            user: None,
            error: Some(error.to_string()),
        }
    }
}

/// SMTP Authenticator
pub struct SmtpAuthenticator {
    db_pool: DatabasePool,
}

impl SmtpAuthenticator {
    pub fn new(db_pool: DatabasePool) -> Self {
        Self { db_pool }
    }

    /// Authenticate using PLAIN mechanism
    ///
    /// PLAIN format: base64(\0username\0password) or base64(authzid\0authcid\0password)
    pub async fn authenticate_plain(&self, credentials: &str) -> AuthResult {
        // Decode base64
        let decoded = match BASE64.decode(credentials.trim()) {
            Ok(d) => d,
            Err(e) => {
                warn!("AUTH PLAIN: Invalid base64: {}", e);
                return AuthResult::failure("Invalid credentials encoding");
            }
        };

        // Parse the decoded string: split by NUL byte
        // Format is: [authzid]\0authcid\0password
        let parts: Vec<&[u8]> = decoded.split(|&b| b == 0).collect();

        let (username, password) = match parts.len() {
            2 => {
                // Format: authcid\0password (authzid is empty)
                (
                    String::from_utf8_lossy(parts[0]).to_string(),
                    String::from_utf8_lossy(parts[1]).to_string(),
                )
            }
            3 => {
                // Format: authzid\0authcid\0password
                // authzid is authorization identity (usually empty or same as authcid)
                // authcid is authentication identity (username)
                let authcid = String::from_utf8_lossy(parts[1]).to_string();
                let password = String::from_utf8_lossy(parts[2]).to_string();
                (authcid, password)
            }
            _ => {
                warn!("AUTH PLAIN: Invalid credential format, got {} parts", parts.len());
                return AuthResult::failure("Invalid credential format");
            }
        };

        debug!("AUTH PLAIN: Attempting authentication for user: {}", username);
        self.verify_credentials(&username, &password).await
    }

    /// Authenticate using LOGIN mechanism (after receiving both username and password)
    pub async fn authenticate_login(&self, username: &str, password: &str) -> AuthResult {
        // Both username and password MUST be base64 encoded per RFC 4616
        let username = match BASE64.decode(username.trim()) {
            Ok(d) => String::from_utf8_lossy(&d).to_string(),
            Err(e) => {
                warn!("AUTH LOGIN: Invalid base64 username: {}", e);
                return AuthResult::failure("Invalid credentials encoding");
            }
        };

        let password = match BASE64.decode(password.trim()) {
            Ok(d) => String::from_utf8_lossy(&d).to_string(),
            Err(e) => {
                warn!("AUTH LOGIN: Invalid base64 password: {}", e);
                return AuthResult::failure("Invalid credentials encoding");
            }
        };

        debug!("AUTH LOGIN: Attempting authentication for user: {}", username);
        self.verify_credentials(&username, &password).await
    }

    /// Verify credentials against the database
    async fn verify_credentials(&self, email: &str, password: &str) -> AuthResult {
        let user_repo = DbUserRepository::new(self.db_pool.clone());

        // Find user by email
        let user = match user_repo.get_by_email(email).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                debug!("AUTH: User not found: {}", email);
                return AuthResult::failure("Authentication failed");
            }
            Err(e) => {
                warn!("AUTH: Database error: {}", e);
                return AuthResult::failure("Temporary authentication error");
            }
        };

        // Check if user is active
        if !user.active {
            debug!("AUTH: User is inactive: {}", email);
            return AuthResult::failure("Authentication failed");
        }

        // Verify password hash
        match self.verify_password(password, &user.password_hash) {
            Ok(true) => {
                debug!("AUTH: Authentication successful for: {}", email);
                AuthResult::success(user)
            }
            Ok(false) => {
                debug!("AUTH: Invalid password for: {}", email);
                AuthResult::failure("Authentication failed")
            }
            Err(e) => {
                warn!("AUTH: Password verification error: {}", e);
                AuthResult::failure("Authentication failed")
            }
        }
    }

    /// Verify password against argon2 hash
    fn verify_password(&self, password: &str, hash: &str) -> Result<bool> {
        // Parse the hash
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| anyhow!("Invalid password hash format: {}", e))?;

        // Verify using default Argon2 parameters
        let argon2 = Argon2::default();

        match argon2.verify_password(password.as_bytes(), &parsed_hash) {
            Ok(_) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(e) => Err(anyhow!("Password verification error: {}", e)),
        }
    }
}

/// Generate base64 encoded challenge for AUTH LOGIN
pub fn login_challenge_username() -> String {
    BASE64.encode(b"Username:")
}

/// Generate base64 encoded challenge for AUTH LOGIN
pub fn login_challenge_password() -> String {
    BASE64.encode(b"Password:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_challenges() {
        assert_eq!(login_challenge_username(), "VXNlcm5hbWU6");
        assert_eq!(login_challenge_password(), "UGFzc3dvcmQ6");
    }

    #[test]
    fn test_base64_decode() {
        // Test PLAIN format: \0user@example.com\0password
        let credentials = BASE64.encode(b"\0user@example.com\0testpass");
        let decoded = BASE64.decode(&credentials).unwrap();
        let parts: Vec<&[u8]> = decoded.split(|&b| b == 0).collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], b"");
        assert_eq!(parts[1], b"user@example.com");
        assert_eq!(parts[2], b"testpass");
    }
}
