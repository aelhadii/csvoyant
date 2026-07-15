//! Integration tests for the auth system, exercised against a real Postgres via `#[sqlx::test]`.
//!
//! Each test gets an isolated, freshly-migrated database. Requires `DATABASE_URL` to point at a
//! Postgres the test user can create databases on (e.g. the compose Postgres).

use api::auth::{AuthState, JwtConfig, auth_router};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

fn app(pool: PgPool) -> Router {
    let state = AuthState {
        pg: pool,
        jwt: JwtConfig::new("test-secret".to_string()),
    };
    auth_router::<AuthState>().with_state(state)
}

/// Send a request and return the status + parsed JSON body (Null if empty/non-JSON).
async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    bearer: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = match body {
        Some(b) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn creds(email: &str) -> Value {
    json!({ "email": email, "password": "supersecret" })
}

#[sqlx::test]
async fn register_returns_tokens_and_me_reflects_the_user(pool: PgPool) {
    let app = app(pool);

    let (status, body) = call(&app, "POST", "/auth/register", Some(creds("a@x.com")), None).await;
    assert_eq!(status, StatusCode::CREATED);
    let access = body["data"]["access_token"].as_str().unwrap().to_string();
    assert!(body["data"]["refresh_token"].as_str().is_some());
    assert!(body["error"].is_null());

    let (status, me) = call(&app, "GET", "/auth/me", None, Some(&access)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["data"]["email"], "a@x.com");
    assert_eq!(me["data"]["role"], "user");
}

#[sqlx::test]
async fn duplicate_email_is_conflict(pool: PgPool) {
    let app = app(pool);
    let (s1, _) = call(
        &app,
        "POST",
        "/auth/register",
        Some(creds("dup@x.com")),
        None,
    )
    .await;
    assert_eq!(s1, StatusCode::CREATED);
    let (s2, body) = call(
        &app,
        "POST",
        "/auth/register",
        Some(creds("dup@x.com")),
        None,
    )
    .await;
    assert_eq!(s2, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
}

#[sqlx::test]
async fn registration_validates_input(pool: PgPool) {
    let app = app(pool);
    let (s1, _) = call(
        &app,
        "POST",
        "/auth/register",
        Some(json!({ "email": "not-an-email", "password": "supersecret" })),
        None,
    )
    .await;
    assert_eq!(s1, StatusCode::UNPROCESSABLE_ENTITY);

    let (s2, _) = call(
        &app,
        "POST",
        "/auth/register",
        Some(json!({ "email": "ok@x.com", "password": "short" })),
        None,
    )
    .await;
    assert_eq!(s2, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn login_succeeds_with_correct_password_and_fails_otherwise(pool: PgPool) {
    let app = app(pool);
    call(
        &app,
        "POST",
        "/auth/register",
        Some(creds("log@x.com")),
        None,
    )
    .await;

    let (ok, body) = call(&app, "POST", "/auth/login", Some(creds("log@x.com")), None).await;
    assert_eq!(ok, StatusCode::OK);
    assert!(body["data"]["access_token"].as_str().is_some());

    let (bad, body) = call(
        &app,
        "POST",
        "/auth/login",
        Some(json!({ "email": "log@x.com", "password": "wrong-password" })),
        None,
    )
    .await;
    assert_eq!(bad, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");

    // Unknown user is the same 401 (no account enumeration).
    let (missing, _) = call(
        &app,
        "POST",
        "/auth/login",
        Some(creds("ghost@x.com")),
        None,
    )
    .await;
    assert_eq!(missing, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn refresh_rotates_and_old_token_is_revoked(pool: PgPool) {
    let app = app(pool);
    let (_, reg) = call(&app, "POST", "/auth/register", Some(creds("r@x.com")), None).await;
    let refresh1 = reg["data"]["refresh_token"].as_str().unwrap().to_string();

    // Exchange refresh1 for a new pair.
    let (status, body) = call(
        &app,
        "POST",
        "/auth/refresh",
        Some(json!({ "refresh_token": refresh1 })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let refresh2 = body["data"]["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(refresh1, refresh2, "refresh token must rotate");

    // refresh1 is now revoked and must be rejected.
    let (reused, _) = call(
        &app,
        "POST",
        "/auth/refresh",
        Some(json!({ "refresh_token": refresh1 })),
        None,
    )
    .await;
    assert_eq!(reused, StatusCode::UNAUTHORIZED);

    // refresh2 still works.
    let (ok, _) = call(
        &app,
        "POST",
        "/auth/refresh",
        Some(json!({ "refresh_token": refresh2 })),
        None,
    )
    .await;
    assert_eq!(ok, StatusCode::OK);
}

#[sqlx::test]
async fn change_email_requires_current_password_and_updates_login(pool: PgPool) {
    let app = app(pool);
    let (_, reg) = call(
        &app,
        "POST",
        "/auth/register",
        Some(creds("old@x.com")),
        None,
    )
    .await;
    let access = reg["data"]["access_token"].as_str().unwrap().to_string();

    // Wrong current password → 401.
    let (bad, _) = call(
        &app,
        "PATCH",
        "/auth/email",
        Some(json!({ "new_email": "new@x.com", "current_password": "wrong" })),
        Some(&access),
    )
    .await;
    assert_eq!(bad, StatusCode::UNAUTHORIZED);

    // Correct current password → 200 and email changes.
    let (ok, body) = call(
        &app,
        "PATCH",
        "/auth/email",
        Some(json!({ "new_email": "new@x.com", "current_password": "supersecret" })),
        Some(&access),
    )
    .await;
    assert_eq!(ok, StatusCode::OK);
    assert_eq!(body["data"]["email"], "new@x.com");

    // Login now works with the new email, not the old one.
    let (with_new, _) = call(&app, "POST", "/auth/login", Some(creds("new@x.com")), None).await;
    assert_eq!(with_new, StatusCode::OK);
    let (with_old, _) = call(&app, "POST", "/auth/login", Some(creds("old@x.com")), None).await;
    assert_eq!(with_old, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn admin_route_enforces_rbac(pool: PgPool) {
    let app = app(pool.clone());

    // No token → 401.
    let (anon, _) = call(&app, "GET", "/admin/ping", None, None).await;
    assert_eq!(anon, StatusCode::UNAUTHORIZED);

    // A normal user is forbidden.
    let (_, reg) = call(&app, "POST", "/auth/register", Some(creds("u@x.com")), None).await;
    let user_token = reg["data"]["access_token"].as_str().unwrap().to_string();
    let (forbidden, body) = call(&app, "GET", "/admin/ping", None, Some(&user_token)).await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");

    // Promote the user to admin, re-login to get an admin-role token, then access is allowed.
    sqlx::query("UPDATE users SET role = 'admin' WHERE lower(email) = 'u@x.com'")
        .execute(&pool)
        .await
        .unwrap();
    let (_, login) = call(&app, "POST", "/auth/login", Some(creds("u@x.com")), None).await;
    let admin_token = login["data"]["access_token"].as_str().unwrap().to_string();
    let (ok, _) = call(&app, "GET", "/admin/ping", None, Some(&admin_token)).await;
    assert_eq!(ok, StatusCode::OK);
}
