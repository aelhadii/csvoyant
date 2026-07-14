//! Authentication & authorization: JWT access tokens, rotating refresh tokens, argon2
//! password hashing, and User/Admin RBAC guards.

pub mod guard;
pub mod handlers;
pub mod jwt;
pub mod models;
pub mod password;
pub mod tokens;

use std::sync::Arc;

use axum::Router;
use axum::extract::FromRef;
use axum::routing::{get, patch, post};
use chrono::Duration;
use sqlx::PgPool;

/// Access-token lifetime.
pub const ACCESS_TTL_MINUTES: i64 = 15;
/// Refresh-token lifetime (matches DECISIONS: 7-day sessions).
pub const REFRESH_TTL_DAYS: i64 = 7;
/// Name of the httpOnly refresh cookie.
pub const REFRESH_COOKIE: &str = "refresh_token";

/// JWT signing config + token lifetimes.
#[derive(Clone)]
pub struct JwtConfig {
    pub secret: Arc<String>,
    pub access_ttl: Duration,
    pub refresh_ttl: Duration,
}

impl JwtConfig {
    pub fn new(secret: String) -> Self {
        Self {
            secret: Arc::new(secret),
            access_ttl: Duration::minutes(ACCESS_TTL_MINUTES),
            refresh_ttl: Duration::days(REFRESH_TTL_DAYS),
        }
    }
}

/// The slice of application state auth handlers and guards need.
#[derive(Clone)]
pub struct AuthState {
    pub pg: PgPool,
    pub jwt: JwtConfig,
}

/// Auth + RBAC routes, generic over any app state that can produce an [`AuthState`].
pub fn auth_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AuthState: FromRef<S>,
{
    Router::new()
        .route("/auth/register", post(handlers::register))
        .route("/auth/login", post(handlers::login))
        .route("/auth/refresh", post(handlers::refresh))
        .route("/auth/email", patch(handlers::change_email))
        .route("/auth/me", get(handlers::me))
        // Admin-only probe route, used to exercise the RBAC guard.
        .route("/admin/ping", get(handlers::admin_ping))
}
