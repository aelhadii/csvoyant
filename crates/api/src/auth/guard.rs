//! Axum extractors that enforce authentication and role-based access.
//!
//! - [`AuthUser`] — succeeds for any valid access token (401 otherwise).
//! - [`AdminUser`] — additionally requires the Admin role (403 otherwise).

use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use shared::Role;
use uuid::Uuid;

use crate::auth::{AuthState, jwt};
use crate::error::AppError;

/// An authenticated caller, resolved from a Bearer access token.
pub struct AuthUser {
    pub user_id: Uuid,
    pub role: Role,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AuthState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = AuthState::from_ref(state);
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("missing authorization header".into()))?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("expected a Bearer token".into()))?;
        let claims = jwt::decode_access(&auth.jwt.secret, token)
            .map_err(|_| AppError::Unauthorized("invalid or expired token".into()))?;
        Ok(AuthUser {
            user_id: claims.sub,
            role: claims.role,
        })
    }
}

/// An authenticated caller that must hold the Admin role.
pub struct AdminUser(#[allow(dead_code)] pub AuthUser);

impl<S> FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
    AuthState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if user.role.satisfies(Role::Admin) {
            Ok(AdminUser(user))
        } else {
            Err(AppError::Forbidden("admin role required".into()))
        }
    }
}
