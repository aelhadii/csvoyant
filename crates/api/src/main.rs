//! CSVoyant API — Axum HTTP server.
//!
//! Thin binary shell: loads config, initializes telemetry, connects downstreams, and serves
//! the router. Handlers and state live in the `api` library crate so tests can reuse them.

use api::auth;
use api::jobs;
use api::state::AppState;
use axum::Router;
use axum::http::{HeaderValue, Method, header};
use axum::routing::get;
use shared::Config;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env in local/dev; ignored if absent (compose injects real env).
    let _ = dotenvy::dotenv();

    let config = Config::from_env()?;
    let _telemetry = shared::telemetry::init("csvoyant-api", &config.telemetry)?;

    let state = AppState::connect(&config).await?;

    // The browser SPA is a different origin (port), so it needs CORS — and credentials must be
    // allowed for the httpOnly refresh cookie to travel. A credentialed request cannot use `*`,
    // so the allowed origin is explicit (see CORS_ALLOWED_ORIGIN).
    let cors = CorsLayer::new()
        .allow_origin(
            config
                .cors_allowed_origin
                .parse::<HeaderValue>()
                .map_err(|e| anyhow::anyhow!("invalid CORS_ALLOWED_ORIGIN: {e}"))?,
        )
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    info!(origin = %config.cors_allowed_origin, "CORS configured");

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .merge(auth::auth_router())
        .merge(jobs::jobs_router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    info!(addr = %config.bind_addr, "api listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Liveness: the process is up and serving. Never touches downstreams.
async fn health() -> &'static str {
    "ok"
}

/// Readiness: every downstream dependency is reachable. Used by compose healthchecks.
async fn ready(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, axum::Json<serde_json::Value>)>
{
    let checks = state.readiness().await;
    let all_ok = checks.iter().all(|(_, ok)| *ok);
    let mut checks_map = serde_json::Map::new();
    for (name, ok) in &checks {
        checks_map.insert((*name).to_string(), serde_json::Value::Bool(*ok));
    }
    let body = axum::Json(serde_json::json!({ "ready": all_ok, "checks": checks_map }));
    if all_ok {
        Ok(body)
    } else {
        Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, body))
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}
