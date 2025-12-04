//! API routes

use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Router,
};
use mairust_storage::DatabasePool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::auth::{auth_middleware, AppState};
use crate::handlers::{domains, health, hooks, mailboxes, messages, send, tenants, users};

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

    // API v1 routes with authentication
    let api_v1 = Router::new()
        .nest("/messages", message_routes)
        .nest("/admin/tenants", tenant_routes)
        .nest("/tenants/:tenant_id/users", user_routes)
        .nest("/tenants/:tenant_id/domains", domain_routes)
        .nest("/tenants/:tenant_id/mailboxes", mailbox_routes)
        .nest("/tenants/:tenant_id/hooks", hook_routes)
        .nest("/tenants/:tenant_id/send", send_routes)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // Combine all routes
    Router::new()
        .nest("/health", health_routes)
        .nest("/api/v1", api_v1)
        .layer(TraceLayer::new_for_http())
}
