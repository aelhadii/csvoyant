//! Request/response DTOs and the persisted user row.

use serde::{Deserialize, Serialize};
use shared::Role;
use uuid::Uuid;
use validator::Validate;

/// Registration payload. Password policy: at least 8 characters.
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(email(message = "must be a valid email address"))]
    pub email: String,
    #[validate(length(min = 8, message = "must be at least 8 characters"))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email(message = "must be a valid email address"))]
    pub email: String,
    #[validate(length(min = 1, message = "is required"))]
    pub password: String,
}

/// Refresh payload. The token may instead arrive via the httpOnly `refresh_token` cookie.
#[derive(Debug, Default, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ChangeEmailRequest {
    #[validate(email(message = "must be a valid email address"))]
    pub new_email: String,
    #[validate(length(min = 1, message = "is required"))]
    pub current_password: String,
}

/// Issued token pair. The access token is always used as `Authorization: Bearer <token>`;
/// `expires_in` is its lifetime in seconds so clients can refresh proactively.
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

/// Public view of a user (never includes the password hash).
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub role: Role,
}

/// A user row as stored in Postgres. `role` is the raw `'user'`/`'admin'` string.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub role: String,
}

impl UserRow {
    /// Domain role, defaulting to `User` if the DB somehow holds an unknown value.
    pub fn role(&self) -> Role {
        Role::parse(&self.role).unwrap_or(Role::User)
    }

    pub fn into_response(self) -> UserResponse {
        let role = self.role();
        UserResponse {
            id: self.id,
            email: self.email,
            role,
        }
    }
}
