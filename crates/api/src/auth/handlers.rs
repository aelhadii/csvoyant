//! Auth endpoint handlers: register, login, refresh (with rotation), change-email, me, admin.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use validator::Validate;

use crate::auth::guard::{AdminUser, AuthUser};
use crate::auth::models::{
    ChangeEmailRequest, LoginRequest, RefreshRequest, RegisterRequest, TokenResponse, UserResponse,
    UserRow,
};
use crate::auth::{AuthState, REFRESH_COOKIE, jwt, password, tokens};
use crate::error::{ApiResult, AppError};

const USER_COLUMNS: &str = "id, email, password_hash, role";

/// POST /auth/register — create a user and issue a token pair.
pub async fn register(
    State(auth): State<AuthState>,
    jar: CookieJar,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, CookieJar, Json<TokenResponse>)> {
    req.validate()?;
    let password_hash = password::hash_password(&req.password)?;

    let row = sqlx::query_as::<_, UserRow>(&format!(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING {USER_COLUMNS}"
    ))
    .bind(&req.email)
    .bind(&password_hash)
    .fetch_one(&auth.pg)
    .await
    .map_err(|e| unique_violation(e, "email already registered"))?;

    let (resp, jar) = issue_tokens(&auth, &row, jar).await?;
    Ok((StatusCode::CREATED, jar, Json(resp)))
}

/// POST /auth/login — verify credentials and issue a token pair.
pub async fn login(
    State(auth): State<AuthState>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> ApiResult<(CookieJar, Json<TokenResponse>)> {
    req.validate()?;

    let row = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE lower(email) = lower($1)"
    ))
    .bind(&req.email)
    .fetch_optional(&auth.pg)
    .await?;

    // Same error whether the user is missing or the password is wrong (no enumeration).
    let row = match row {
        Some(r) if password::verify_password(&req.password, &r.password_hash) => r,
        _ => return Err(AppError::Unauthorized("invalid credentials".into())),
    };

    let (resp, jar) = issue_tokens(&auth, &row, jar).await?;
    Ok((jar, Json(resp)))
}

/// POST /auth/refresh — rotate a refresh token: revoke the old, issue a new pair.
/// The refresh token may arrive in the JSON body or the httpOnly cookie.
pub async fn refresh(
    State(auth): State<AuthState>,
    jar: CookieJar,
    body: Option<Json<RefreshRequest>>,
) -> ApiResult<(CookieJar, Json<TokenResponse>)> {
    let presented = body
        .and_then(|Json(b)| b.refresh_token)
        .or_else(|| jar.get(REFRESH_COOKIE).map(|c| c.value().to_string()))
        .ok_or_else(|| AppError::Unauthorized("missing refresh token".into()))?;

    let token_hash = tokens::hash_token(&presented);

    // A valid token is present, not revoked, and not expired.
    let found = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT id, user_id FROM refresh_tokens \
         WHERE token_hash = $1 AND revoked = false AND expires_at > now()",
    )
    .bind(&token_hash)
    .fetch_optional(&auth.pg)
    .await?;

    let (token_id, user_id) =
        found.ok_or_else(|| AppError::Unauthorized("invalid or expired refresh token".into()))?;

    // Rotation: revoke the presented token so it can't be reused.
    sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE id = $1")
        .bind(token_id)
        .execute(&auth.pg)
        .await?;

    let user =
        sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
            .bind(user_id)
            .fetch_one(&auth.pg)
            .await?;

    let (resp, jar) = issue_tokens(&auth, &user, jar).await?;
    Ok((jar, Json(resp)))
}

/// PATCH /auth/email — change the authenticated user's email (re-auth with current password).
pub async fn change_email(
    State(auth): State<AuthState>,
    user: AuthUser,
    Json(req): Json<ChangeEmailRequest>,
) -> ApiResult<Json<UserResponse>> {
    req.validate()?;

    let current =
        sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
            .bind(user.user_id)
            .fetch_one(&auth.pg)
            .await?;

    if !password::verify_password(&req.current_password, &current.password_hash) {
        return Err(AppError::Unauthorized("invalid credentials".into()));
    }

    let updated = sqlx::query_as::<_, UserRow>(&format!(
        "UPDATE users SET email = $1, updated_at = now() WHERE id = $2 RETURNING {USER_COLUMNS}"
    ))
    .bind(&req.new_email)
    .bind(user.user_id)
    .fetch_one(&auth.pg)
    .await
    .map_err(|e| unique_violation(e, "email already registered"))?;

    Ok(Json(updated.into_response()))
}

/// GET /auth/me — the authenticated user's profile.
pub async fn me(State(auth): State<AuthState>, user: AuthUser) -> ApiResult<Json<UserResponse>> {
    let row =
        sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
            .bind(user.user_id)
            .fetch_optional(&auth.pg)
            .await?
            .ok_or_else(|| AppError::NotFound("user not found".into()))?;
    Ok(Json(row.into_response()))
}

/// GET /admin/ping — Admin-only; proves the RBAC guard rejects non-admins.
pub async fn admin_ping(_admin: AdminUser) -> &'static str {
    "pong"
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Mint an access token + a fresh rotating refresh token, persist the refresh hash, and
/// attach the refresh token as an httpOnly cookie.
async fn issue_tokens(
    auth: &AuthState,
    user: &UserRow,
    jar: CookieJar,
) -> ApiResult<(TokenResponse, CookieJar)> {
    let access_token =
        jwt::encode_access(&auth.jwt.secret, user.id, user.role(), auth.jwt.access_ttl)?;

    let refresh_token = tokens::generate_refresh_token();
    let refresh_hash = tokens::hash_token(&refresh_token);
    let expires_at: DateTime<Utc> = Utc::now() + auth.jwt.refresh_ttl;

    sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)")
        .bind(user.id)
        .bind(&refresh_hash)
        .bind(expires_at)
        .execute(&auth.pg)
        .await?;

    let jar = jar.add(refresh_cookie(
        refresh_token.clone(),
        auth.jwt.refresh_ttl.num_seconds(),
    ));
    let resp = TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        expires_in: auth.jwt.access_ttl.num_seconds(),
    };
    Ok((resp, jar))
}

fn refresh_cookie(value: String, max_age_secs: i64) -> Cookie<'static> {
    Cookie::build((REFRESH_COOKIE, value))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        // secure(false) for local http; a TLS-terminating deployment should set this true.
        .secure(false)
        .max_age(time::Duration::seconds(max_age_secs))
        .build()
}

/// Map a unique-constraint violation to a 409 Conflict; anything else stays a 500.
fn unique_violation(e: sqlx::Error, message: &str) -> AppError {
    if let sqlx::Error::Database(db) = &e
        && db.is_unique_violation()
    {
        return AppError::Conflict(message.to_string());
    }
    AppError::Internal(e.into())
}
