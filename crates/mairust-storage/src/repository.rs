//! Repository layer for data access

pub mod api_keys;
pub mod messages;
pub mod tenants;
pub mod users;
pub mod mailboxes;
pub mod domains;
pub mod hooks;
pub mod domain_aliases;
pub mod domain_settings;
pub mod policies;
pub mod threads;
pub mod tags;
pub mod categories;

// Re-export concrete repository implementations with simple names
pub use api_keys::DbApiKeyRepository as ApiKeyRepository;
pub use messages::DbMessageRepository as MessageRepository;
pub use tenants::DbTenantRepository as TenantRepository;
pub use users::DbUserRepository as UserRepository;
pub use mailboxes::DbMailboxRepository as MailboxRepository;
pub use domains::DbDomainRepository as DomainRepository;
pub use hooks::DbHookRepository as HookRepository;
pub use domain_aliases::DbDomainAliasRepository as DomainAliasRepository;
pub use domain_settings::DbDomainSettingsRepository as DomainSettingsRepository;
pub use policies::DbPolicyRepository as PolicyRepository;
pub use threads::ThreadRepository;
pub use tags::TagRepository;
pub use categories::CategoryRepository;

// Re-export repository traits
pub use api_keys::ApiKeyRepository as ApiKeyRepositoryTrait;
pub use domains::DomainRepository as DomainRepositoryTrait;
pub use hooks::HookRepository as HookRepositoryTrait;
pub use mailboxes::MailboxRepository as MailboxRepositoryTrait;
pub use domain_aliases::DomainAliasRepository as DomainAliasRepositoryTrait;
pub use domain_settings::DomainSettingsRepository as DomainSettingsRepositoryTrait;
pub use policies::PolicyRepository as PolicyRepositoryTrait;

// Re-export API key types
pub use api_keys::{ApiKey, ApiKeyId};
