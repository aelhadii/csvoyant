//! CSVoyant API — Axum HTTP server.
//!
//! Thin binary shell: loads config, initializes telemetry, connects downstreams, and serves
//! the router. Handlers and state live in the `api` library crate so tests can reuse them.

use api::auth;
use api::state::AppState;
use axum::Router;
use axum::routing::get;
use shared::Config;
use tower_http::trace::TraceLayer;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env in local/dev; ignored if absent (compose injects real env).
    let _ = dotenvy::dotenv();

    let config = Config::from_env()?;
    let _telemetry = shared::telemetry::init("csvoyant-api", &config.telemetry)?;

    let state = AppState::connect(&config).await?;

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .merge(auth::auth_router())
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
