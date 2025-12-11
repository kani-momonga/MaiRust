//! API routes

use axum::{
    middleware,
    routing::{delete, get, patch, post, put},
    Router,
};
use mairust_storage::DatabasePool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::auth::{auth_middleware, AppState};
use crate::handlers::{
    admin, campaigns, domain_aliases, domain_settings, domains, health, hooks, mailboxes, messages,
    policies, recipient_lists, search, send, tenants, users,
};
use crate::openapi::create_openapi_routes;

/// Create the API router
pub fn create_router(db_pool: DatabasePool) -> Router {
    let state = Arc::new(AppState { db_pool });

    // Health check routes (no auth required)
    let health_routes = Router::new()
        .route("/", get(health::health))
        .route("/live", get(health::liveness))
        .route("/ready", get(health::readiness))
        .route("/detailed", get(health::health_detailed))
        .with_state(state.clone());

    // Message routes
    let message_routes = Router::new()
        .route("/", get(messages::list_messages))
        .route("/:id", get(messages::get_message))
        .route("/:id/flags", patch(messages::update_message_flags))
        .route("/:id", delete(messages::delete_message));

    // Tenant routes (admin)
    let tenant_routes = Router::new()
        .route("/", get(tenants::list_tenants))
        .route("/", post(tenants::create_tenant))
        .route("/:id", get(tenants::get_tenant))
        .route("/:id", delete(tenants::delete_tenant));

    // User routes
    let user_routes = Router::new()
        .route("/", get(users::list_users))
        .route("/", post(users::create_user))
        .route("/:id", get(users::get_user))
        .route("/:id", delete(users::delete_user));

    // Domain routes
    let domain_routes = Router::new()
        .route("/", get(domains::list_domains))
        .route("/", post(domains::create_domain))
        .route("/:domain_id", get(domains::get_domain))
        .route("/:domain_id", delete(domains::delete_domain))
        .route("/:domain_id/verify", post(domains::verify_domain))
        .route("/:domain_id/dkim", post(domains::set_dkim));

    // Mailbox routes
    let mailbox_routes = Router::new()
        .route("/", get(mailboxes::list_mailboxes))
        .route("/", post(mailboxes::create_mailbox))
        .route("/:mailbox_id", get(mailboxes::get_mailbox))
        .route("/:mailbox_id", delete(mailboxes::delete_mailbox))
        .route("/:mailbox_id/quota", patch(mailboxes::update_mailbox_quota));

    // Hook routes
    let hook_routes = Router::new()
        .route("/", get(hooks::list_hooks))
        .route("/", post(hooks::create_hook))
        .route("/:hook_id", get(hooks::get_hook))
        .route("/:hook_id", delete(hooks::delete_hook))
        .route("/:hook_id/enable", post(hooks::enable_hook))
        .route("/:hook_id/disable", post(hooks::disable_hook));

    // Send routes
    let send_routes = Router::new()
        .route("/", post(send::send_email))
        .route("/queue", get(send::get_send_queue))
        .route("/:message_id/status", get(send::get_message_status));

    // Domain alias routes
    let domain_alias_routes = Router::new()
        .route("/", get(domain_aliases::list_domain_aliases))
        .route("/", post(domain_aliases::create_domain_alias))
        .route("/:alias_id", get(domain_aliases::get_domain_alias))
        .route("/:alias_id", delete(domain_aliases::delete_domain_alias))
        .route("/:alias_id/enable", post(domain_aliases::enable_domain_alias))
        .route("/:alias_id/disable", post(domain_aliases::disable_domain_alias));

    // Domain settings routes
    let domain_settings_routes = Router::new()
        .route("/", get(domain_settings::get_domain_settings))
        .route("/", put(domain_settings::update_domain_settings))
        .route("/catch-all", post(domain_settings::enable_catch_all))
        .route("/catch-all", delete(domain_settings::disable_catch_all));

    // Policy routes
    let policy_routes = Router::new()
        .route("/", get(policies::list_policies))
        .route("/", post(policies::create_policy))
        .route("/:policy_id", get(policies::get_policy))
        .route("/:policy_id", put(policies::update_policy))
        .route("/:policy_id", delete(policies::delete_policy))
        .route("/:policy_id/enable", post(policies::enable_policy))
        .route("/:policy_id/disable", post(policies::disable_policy));

    // Search routes
    let search_routes = Router::new()
        .route("/", get(search::search_messages))
        .route("/status", get(search::search_status))
        .route("/reindex", post(search::reindex_messages));

    // Campaign routes
    let campaign_routes = Router::new()
        .route("/", get(campaigns::list_campaigns))
        .route("/", post(campaigns::create_campaign))
        .route("/:campaign_id", get(campaigns::get_campaign))
        .route("/:campaign_id", put(campaigns::update_campaign))
        .route("/:campaign_id", delete(campaigns::delete_campaign))
        .route("/:campaign_id/schedule", post(campaigns::schedule_campaign))
        .route("/:campaign_id/send", post(campaigns::send_campaign))
        .route("/:campaign_id/pause", post(campaigns::pause_campaign))
        .route("/:campaign_id/resume", post(campaigns::resume_campaign))
        .route("/:campaign_id/cancel", post(campaigns::cancel_campaign))
        .route("/:campaign_id/stats", get(campaigns::get_campaign_stats));

    // Recipient list routes
    let recipient_list_routes = Router::new()
        .route("/", get(recipient_lists::list_recipient_lists))
        .route("/", post(recipient_lists::create_recipient_list))
        .route("/:list_id", get(recipient_lists::get_recipient_list))
        .route("/:list_id", put(recipient_lists::update_recipient_list))
        .route("/:list_id", delete(recipient_lists::delete_recipient_list))
        .route("/:list_id/recipients", get(recipient_lists::list_recipients))
        .route("/:list_id/recipients", post(recipient_lists::add_recipient))
        .route("/:list_id/recipients/import", post(recipient_lists::import_recipients))
        .route("/:list_id/recipients/:recipient_id", get(recipient_lists::get_recipient))
        .route("/:list_id/recipients/:recipient_id", put(recipient_lists::update_recipient))
        .route("/:list_id/recipients/:recipient_id", delete(recipient_lists::delete_recipient));

    // Admin dashboard routes (super admin)
    let admin_system_routes = Router::new()
        .route("/stats", get(admin::get_system_stats))
        .route("/tenants", get(admin::list_all_tenants_summary));

    // Tenant admin routes
    let tenant_admin_routes = Router::new()
        .route("/usage", get(admin::get_tenant_usage))
        .route("/audit-logs", get(admin::list_audit_logs));

    // API v1 routes with authentication
    let api_v1 = Router::new()
        .nest("/messages", message_routes)
        .nest("/admin/tenants", tenant_routes)
        .nest("/admin/system", admin_system_routes)
        .nest("/tenants/:tenant_id/admin", tenant_admin_routes)
        .nest("/tenants/:tenant_id/users", user_routes)
        .nest("/tenants/:tenant_id/domains", domain_routes)
        .nest("/tenants/:tenant_id/domains/:domain_id/settings", domain_settings_routes)
        .nest("/tenants/:tenant_id/domain-aliases", domain_alias_routes)
        .nest("/tenants/:tenant_id/mailboxes", mailbox_routes)
        .nest("/tenants/:tenant_id/hooks", hook_routes)
        .nest("/tenants/:tenant_id/policies", policy_routes)
        .nest("/tenants/:tenant_id/search", search_routes)
        .nest("/tenants/:tenant_id/send", send_routes)
        .nest("/tenants/:tenant_id/campaigns", campaign_routes)
        .nest("/tenants/:tenant_id/recipient-lists", recipient_list_routes)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // OpenAPI documentation routes
    let openapi_routes = create_openapi_routes();

    // Combine all routes
    Router::new()
        .nest("/health", health_routes)
        .nest("/api/v1", api_v1)
        .merge(openapi_routes)
        .layer(TraceLayer::new_for_http())
}
