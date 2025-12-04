//! Repository layer for data access

pub mod messages;
pub mod tenants;
pub mod users;
pub mod mailboxes;
pub mod domains;
pub mod hooks;

// Re-export concrete repository implementations with simple names
pub use messages::DbMessageRepository as MessageRepository;
pub use tenants::DbTenantRepository as TenantRepository;
pub use users::DbUserRepository as UserRepository;
pub use mailboxes::DbMailboxRepository as MailboxRepository;
pub use domains::DbDomainRepository as DomainRepository;
pub use hooks::DbHookRepository as HookRepository;

// Re-export repository traits
pub use domains::DomainRepository as DomainRepositoryTrait;
pub use hooks::HookRepository as HookRepositoryTrait;
pub use mailboxes::MailboxRepository as MailboxRepositoryTrait;
