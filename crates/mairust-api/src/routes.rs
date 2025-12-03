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
use crate::handlers::{health, messages, tenants, users};

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

    // API v1 routes with authentication
    let api_v1 = Router::new()
        .nest("/messages", message_routes)
        .nest("/admin/tenants", tenant_routes)
        .nest("/tenants/:tenant_id/users", user_routes)
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
