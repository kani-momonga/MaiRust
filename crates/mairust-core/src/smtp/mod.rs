//! SMTP server module

mod handler;
mod server;

pub use handler::SmtpHandler;
pub use server::SmtpServer;
