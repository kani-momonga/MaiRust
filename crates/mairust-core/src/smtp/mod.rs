//! SMTP server module

mod auth;
mod handler;
mod server;
mod tls;

pub use auth::{AuthResult, SmtpAuthenticator};
pub use handler::SmtpHandler;
pub use server::{SmtpServer, SmtpServiceType};
pub use tls::{create_tls_acceptor, is_tls_configured};
