//! Web UI Handlers
//!
//! Request handlers for the web UI.

use crate::{AppState, StaticAssets};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde::Deserialize;
use time::Duration;
use uuid::Uuid;

/// Session cookie name
const SESSION_COOKIE: &str = "mairust_session";

/// Session duration in hours
const SESSION_DURATION_HOURS: i64 = 24;

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

/// Check if user is authenticated via session cookie
async fn check_auth(state: &AppState, jar: &CookieJar) -> Option<(Uuid, Uuid, String)> {
    let session_id = jar.get(SESSION_COOKIE)?.value().to_string();

    // Query session from database
    let pool = state.db_pool.pool();
    let session: Option<(Uuid, Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT user_id, tenant_id, expires_at FROM sessions WHERE id = $1",
    )
    .bind(&session_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match session {
        Some((user_id, tenant_id, expires_at)) => {
            // Check if session is expired
            if expires_at < chrono::Utc::now() {
                // Delete expired session
                let _ = sqlx::query("DELETE FROM sessions WHERE id = $1")
                    .bind(&session_id)
                    .execute(pool)
                    .await;
                return None;
            }

            // Get user email
            let user: Option<(String,)> = sqlx::query_as(
                "SELECT email FROM users WHERE id = $1",
            )
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

            user.map(|(email,)| (user_id, tenant_id, email))
        }
        None => None,
    }
}

/// Index page - redirects to inbox or login
pub async fn index(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if check_auth(&state, &jar).await.is_some() {
        Redirect::to("/inbox")
    } else {
        Redirect::to("/login")
    }
}

/// Inbox page
pub async fn inbox(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    let user = match check_auth(&state, &jar).await {
        Some(u) => u,
        None => return Redirect::to("/login").into_response(),
    };

    let context = serde_json::json!({
        "title": "Inbox",
        "active_page": "inbox",
        "api_url": state.config.api_url,
        "user_email": user.2,
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
pub async fn compose(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    let user = match check_auth(&state, &jar).await {
        Some(u) => u,
        None => return Redirect::to("/login").into_response(),
    };

    let context = serde_json::json!({
        "title": "Compose",
        "active_page": "compose",
        "api_url": state.config.api_url,
        "user_email": user.2,
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
    jar: CookieJar,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match check_auth(&state, &jar).await {
        Some(u) => u,
        None => return Redirect::to("/login").into_response(),
    };

    let context = serde_json::json!({
        "title": "Message",
        "active_page": "inbox",
        "message_id": id,
        "api_url": state.config.api_url,
        "user_email": user.2,
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
pub async fn settings(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    let user = match check_auth(&state, &jar).await {
        Some(u) => u,
        None => return Redirect::to("/login").into_response(),
    };

    let context = serde_json::json!({
        "title": "Settings",
        "active_page": "settings",
        "api_url": state.config.api_url,
        "user_email": user.2,
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
pub async fn login_page(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    // If already logged in, redirect to inbox
    if check_auth(&state, &jar).await.is_some() {
        return Redirect::to("/inbox").into_response();
    }

    let context = serde_json::json!({
        "title": "Login",
        "api_url": state.config.api_url,
        "error": Option::<String>::None,
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
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    tracing::info!("Login attempt for: {}", form.email);

    let pool = state.db_pool.pool();

    // Query user by email
    let user: Option<(Uuid, Uuid, String, bool)> = sqlx::query_as(
        "SELECT id, tenant_id, password_hash, active FROM users WHERE email = $1",
    )
    .bind(&form.email)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match user {
        Some((user_id, tenant_id, password_hash, active)) => {
            if !active {
                tracing::warn!("Login attempt for disabled account: {}", form.email);
                return render_login_error(&state, "Account is disabled").into_response();
            }

            // Verify password using argon2
            let password_valid = if let Ok(parsed_hash) = PasswordHash::new(&password_hash) {
                Argon2::default()
                    .verify_password(form.password.as_bytes(), &parsed_hash)
                    .is_ok()
            } else {
                false
            };

            if !password_valid {
                tracing::warn!("Invalid password for: {}", form.email);
                return render_login_error(&state, "Invalid email or password").into_response();
            }

            // Create session
            let session_id = Uuid::new_v4().to_string();
            let expires_at = chrono::Utc::now() + chrono::Duration::hours(SESSION_DURATION_HOURS);

            let insert_result = sqlx::query(
                "INSERT INTO sessions (id, user_id, tenant_id, expires_at, created_at)
                 VALUES ($1, $2, $3, $4, NOW())",
            )
            .bind(&session_id)
            .bind(user_id)
            .bind(tenant_id)
            .bind(expires_at)
            .execute(pool)
            .await;

            if let Err(e) = insert_result {
                tracing::error!("Failed to create session: {}", e);
                return render_login_error(&state, "Login failed, please try again").into_response();
            }

            tracing::info!("User {} logged in successfully", form.email);

            // Set session cookie
            let cookie = Cookie::build((SESSION_COOKIE, session_id))
                .path("/")
                .max_age(Duration::hours(SESSION_DURATION_HOURS))
                .http_only(true)
                .same_site(axum_extra::extract::cookie::SameSite::Lax)
                .build();

            (jar.add(cookie), Redirect::to("/inbox")).into_response()
        }
        None => {
            tracing::warn!("Unknown user: {}", form.email);
            render_login_error(&state, "Invalid email or password").into_response()
        }
    }
}

/// Render login page with error
fn render_login_error(state: &AppState, error: &str) -> Html<String> {
    let context = serde_json::json!({
        "title": "Login",
        "api_url": state.config.api_url,
        "error": error,
    });

    match state.templates.render("login", &context) {
        Ok(html) => Html(html),
        Err(_) => Html("<h1>Login Error</h1>".to_string()),
    }
}

/// Logout handler
pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    // Delete session from database if exists
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let session_id = cookie.value();
        let pool = state.db_pool.pool();

        let _ = sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(session_id)
            .execute(pool)
            .await;
    }

    // Remove cookie
    let cookie = Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .max_age(Duration::seconds(0))
        .http_only(true)
        .build();

    (jar.remove(cookie), Redirect::to("/login")).into_response()
}
