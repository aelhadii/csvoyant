//! Integration tests for the job read APIs, focused on **per-tenant isolation**: a User may only
//! reach their own jobs/dashboards/data; an Admin may reach any. Runs against a real Postgres via
//! `#[sqlx::test]` (isolated, freshly-migrated DB per test). Requires `DATABASE_URL`.
//!
//! ClickHouse is never contacted here: every assertion is about tenancy or request validation,
//! both of which are decided before any dataset query is issued.

use std::sync::Arc;
use std::time::Duration;

use api::auth::{AuthState, JwtConfig, auth_router};
use api::jobs::{JobsState, jobs_router};
use axum::Router;
use axum::body::Body;
use axum::extract::FromRef;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use shared::{ChHttp, Config, TelemetryConfig};
use sqlx::PgPool;
use tokio::sync::Notify;
use tower::ServiceExt;
use uuid::Uuid;

/// A state that satisfies both routers, standing in for the real `AppState` (which would need a
/// live RabbitMQ connection).
#[derive(Clone)]
struct TestState {
    auth: AuthState,
    jobs: JobsState,
}

impl FromRef<TestState> for AuthState {
    fn from_ref(s: &TestState) -> Self {
        s.auth.clone()
    }
}

impl FromRef<TestState> for JobsState {
    fn from_ref(s: &TestState) -> Self {
        s.jobs.clone()
    }
}

/// A config pointing ClickHouse at a dead port — these tests must never reach it.
fn test_config() -> Config {
    Config {
        database_url: "postgres://unused".into(),
        clickhouse_url: "http://127.0.0.1:1".into(),
        clickhouse_user: "default".into(),
        clickhouse_password: String::new(),
        clickhouse_database: "default".into(),
        amqp_url: "amqp://unused".into(),
        jwt_secret: "test-secret".into(),
        bind_addr: "0.0.0.0:0".into(),
        telemetry: TelemetryConfig {
            otlp_endpoint: String::new(),
            axiom_token: String::new(),
            axiom_dataset: String::new(),
        },
    }
}

fn app(pool: PgPool) -> Router {
    let state = TestState {
        auth: AuthState {
            pg: pool.clone(),
            jwt: JwtConfig::new("test-secret".to_string()),
        },
        jobs: JobsState {
            pg: pool,
            relay_notify: Arc::new(Notify::new()),
            ch: ChHttp::new(&test_config(), Duration::from_secs(1)).unwrap(),
        },
    };
    auth_router::<TestState>()
        .merge(jobs_router::<TestState>())
        .with_state(state)
}

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

/// Register a user and return (access token, user id).
async fn register(app: &Router, email: &str) -> (String, Uuid) {
    let (status, body) = call(
        app,
        "POST",
        "/auth/register",
        Some(json!({ "email": email, "password": "supersecret" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let token = body["data"]["access_token"].as_str().unwrap().to_string();
    let (_, me) = call(app, "GET", "/auth/me", None, Some(&token)).await;
    let id = Uuid::parse_str(me["data"]["id"].as_str().unwrap()).unwrap();
    (token, id)
}

/// Insert a job already in the `ready` state (as the worker would leave it).
async fn insert_ready_job(pool: &PgPool, user_id: Uuid) -> Uuid {
    let schema = json!({
        "columns": [
            { "name": "id", "type": "Int64" },
            { "name": "name", "type": "String" }
        ]
    });
    sqlx::query_scalar(
        "INSERT INTO ingestion_jobs \
           (user_id, source_url, status, clickhouse_table, row_count, inferred_schema, finished_at) \
         VALUES ($1, $2, 'ready', $3, $4, $5, now()) RETURNING id",
    )
    .bind(user_id)
    .bind("https://example.com/data.csv")
    .bind("u_test_j_test")
    .bind(2_i64)
    .bind(schema)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_dashboard(pool: &PgPool, job_id: Uuid, user_id: Uuid) {
    sqlx::query("INSERT INTO dashboards (job_id, user_id, config) VALUES ($1, $2, $3)")
        .bind(job_id)
        .bind(user_id)
        .bind(json!({ "summary": { "row_count": 2, "column_count": 2 }, "columns": [], "charts": [] }))
        .execute(pool)
        .await
        .unwrap();
}

async fn promote_to_admin(pool: &PgPool, email: &str) {
    sqlx::query("UPDATE users SET role = 'admin' WHERE lower(email) = lower($1)")
        .bind(email)
        .execute(pool)
        .await
        .unwrap();
}

// ── tenancy ──────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn owner_can_read_their_job_but_another_user_gets_404(pool: PgPool) {
    let app = app(pool.clone());
    let (alice, alice_id) = register(&app, "alice@x.com").await;
    let (bob, _) = register(&app, "bob@x.com").await;
    let job = insert_ready_job(&pool, alice_id).await;

    let (owner, body) = call(&app, "GET", &format!("/jobs/{job}"), None, Some(&alice)).await;
    assert_eq!(owner, StatusCode::OK);
    assert_eq!(body["data"]["status"], "ready");

    // Cross-tenant reads are 404 (not 403) so the job's existence isn't leaked.
    let (other, body) = call(&app, "GET", &format!("/jobs/{job}"), None, Some(&bob)).await;
    assert_eq!(other, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[sqlx::test]
async fn cross_tenant_dashboard_and_data_are_denied(pool: PgPool) {
    let app = app(pool.clone());
    let (alice, alice_id) = register(&app, "alice@x.com").await;
    let (bob, _) = register(&app, "bob@x.com").await;
    let job = insert_ready_job(&pool, alice_id).await;
    insert_dashboard(&pool, job, alice_id).await;

    // Owner can read the dashboard.
    let (ok, body) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/dashboard"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(ok, StatusCode::OK);
    assert_eq!(body["data"]["summary"]["row_count"], 2);

    // Another user cannot — for either sub-resource.
    let (d, _) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/dashboard"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(d, StatusCode::NOT_FOUND);

    let (data, _) = call(&app, "GET", &format!("/jobs/{job}/data"), None, Some(&bob)).await;
    assert_eq!(data, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_jobs_is_scoped_to_the_caller(pool: PgPool) {
    let app = app(pool.clone());
    let (alice, alice_id) = register(&app, "alice@x.com").await;
    let (bob, _) = register(&app, "bob@x.com").await;
    insert_ready_job(&pool, alice_id).await;

    let (_, mine) = call(&app, "GET", "/jobs", None, Some(&alice)).await;
    assert_eq!(mine["data"].as_array().unwrap().len(), 1);

    let (_, theirs) = call(&app, "GET", "/jobs", None, Some(&bob)).await;
    assert!(theirs["data"].as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn admin_can_read_any_users_job_and_list(pool: PgPool) {
    let app = app(pool.clone());
    let (_, alice_id) = register(&app, "alice@x.com").await;
    register(&app, "root@x.com").await;
    let job = insert_ready_job(&pool, alice_id).await;
    insert_dashboard(&pool, job, alice_id).await;

    // Promote and re-login so the token carries the admin role.
    promote_to_admin(&pool, "root@x.com").await;
    let (_, login) = call(
        &app,
        "POST",
        "/auth/login",
        Some(json!({ "email": "root@x.com", "password": "supersecret" })),
        None,
    )
    .await;
    let admin = login["data"]["access_token"].as_str().unwrap().to_string();

    let (job_status, _) = call(&app, "GET", &format!("/jobs/{job}"), None, Some(&admin)).await;
    assert_eq!(job_status, StatusCode::OK);

    let (dash, _) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/dashboard"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(dash, StatusCode::OK);

    // Admin listing spans all users.
    let (_, all) = call(&app, "GET", "/jobs", None, Some(&admin)).await;
    assert_eq!(all["data"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn unauthenticated_access_is_rejected(pool: PgPool) {
    let app = app(pool.clone());
    let (_, alice_id) = register(&app, "alice@x.com").await;
    let job = insert_ready_job(&pool, alice_id).await;

    for uri in [
        format!("/jobs/{job}"),
        format!("/jobs/{job}/dashboard"),
        format!("/jobs/{job}/data"),
        "/jobs".to_string(),
    ] {
        let (status, _) = call(&app, "GET", &uri, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} must require auth");
    }
}

// ── dashboard / data behaviour ───────────────────────────────────────────────

#[sqlx::test]
async fn dashboard_is_404_until_it_is_generated(pool: PgPool) {
    let app = app(pool.clone());
    let (alice, alice_id) = register(&app, "alice@x.com").await;
    let job = insert_ready_job(&pool, alice_id).await; // no dashboard row yet

    let (status, body) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/dashboard"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[sqlx::test]
async fn data_requires_a_ready_job(pool: PgPool) {
    let app = app(pool.clone());
    let (alice, alice_id) = register(&app, "alice@x.com").await;
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO ingestion_jobs (user_id, source_url, status) VALUES ($1, $2, 'queued') RETURNING id",
    )
    .bind(alice_id)
    .bind("https://example.com/data.csv")
    .fetch_one(&pool)
    .await
    .unwrap();

    let (status, body) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/data"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not ready")
    );
}

#[sqlx::test]
async fn data_rejects_unknown_columns_and_bad_params(pool: PgPool) {
    let app = app(pool.clone());
    let (alice, alice_id) = register(&app, "alice@x.com").await;
    let job = insert_ready_job(&pool, alice_id).await;

    // Sorting by a column that isn't in the schema is rejected (identifier injection guard).
    let (sort, body) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/data?sort=evil"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(sort, StatusCode::BAD_REQUEST);
    assert!(body["error"]["message"].as_str().unwrap().contains("evil"));

    // Filtering on an unknown column likewise.
    let (filter, _) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/data?filter=evil:x"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(filter, StatusCode::BAD_REQUEST);

    // Filter must be `column:value`.
    let (malformed, _) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/data?filter=nocolon"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(malformed, StatusCode::BAD_REQUEST);

    // Order must be asc/desc.
    let (order, _) = call(
        &app,
        "GET",
        &format!("/jobs/{job}/data?sort=id&order=sideways"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(order, StatusCode::BAD_REQUEST);
}
