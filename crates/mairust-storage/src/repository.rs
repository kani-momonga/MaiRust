//! Repository layer for data access

pub mod messages;
pub mod tenants;
pub mod users;
pub mod mailboxes;
pub mod domains;
pub mod hooks;

pub use messages::MessageRepository;
pub use tenants::TenantRepository;
pub use users::UserRepository;
pub use mailboxes::MailboxRepository;
pub use domains::DomainRepository;
pub use hooks::HookRepository;
