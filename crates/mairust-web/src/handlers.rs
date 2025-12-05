//! Web UI Handlers
//!
//! Request handlers for the web UI.

use crate::{AppState, StaticAssets};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use serde::Deserialize;

/// Health check handler
pub async fn health() -> impl IntoResponse {
    "OK"
}

/// Serve static files
pub async fn static_file(Path(path): Path<String>) -> impl IntoResponse {
    match StaticAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Index page - redirects to inbox
pub async fn index() -> impl IntoResponse {
    Redirect::to("/inbox")
}

/// Inbox page
pub async fn inbox(State(state): State<AppState>) -> impl IntoResponse {
    let context = serde_json::json!({
        "title": "Inbox",
        "active_page": "inbox",
        "api_url": state.config.api_url,
    });

    match state.templates.render("inbox", &context) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Template error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Compose page
pub async fn compose(State(state): State<AppState>) -> impl IntoResponse {
    let context = serde_json::json!({
        "title": "Compose",
        "active_page": "compose",
        "api_url": state.config.api_url,
    });

    match state.templates.render("compose", &context) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Template error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Message view page
pub async fn message(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let context = serde_json::json!({
        "title": "Message",
        "active_page": "inbox",
        "message_id": id,
        "api_url": state.config.api_url,
    });

    match state.templates.render("message", &context) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Template error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Settings page
pub async fn settings(State(state): State<AppState>) -> impl IntoResponse {
    let context = serde_json::json!({
        "title": "Settings",
        "active_page": "settings",
        "api_url": state.config.api_url,
    });

    match state.templates.render("settings", &context) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Template error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Login page
pub async fn login_page(State(state): State<AppState>) -> impl IntoResponse {
    let context = serde_json::json!({
        "title": "Login",
        "api_url": state.config.api_url,
    });

    match state.templates.render("login", &context) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Template error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Login form data
#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

/// Login form submission
pub async fn login_submit(
    State(_state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    // TODO: Implement actual authentication
    // For now, just redirect to inbox
    tracing::info!("Login attempt for: {}", form.email);

    Redirect::to("/inbox")
}

/// Logout handler
pub async fn logout() -> impl IntoResponse {
    // TODO: Clear session
    Redirect::to("/login")
}
