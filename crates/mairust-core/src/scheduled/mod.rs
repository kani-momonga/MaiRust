//! Scheduled Email Module - Campaign management and scheduled delivery

mod manager;
mod scheduler;
mod rate_limiter;
mod template;

pub use manager::{CampaignManager, CampaignError};
pub use scheduler::{ScheduledDeliveryWorker, DeliveryResult, SmtpConfig};
pub use rate_limiter::{RateLimiter, RemainingQuota};
pub use template::TemplateRenderer;
