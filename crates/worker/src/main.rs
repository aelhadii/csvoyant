//! CSVoyant worker — RabbitMQ consumer + ingestion pipeline.
//!
//! Prompt A scope: a runnable skeleton. It loads config, initializes telemetry, connects to
//! RabbitMQ and declares the durable topology (jobs queue + dead-letter exchange/queue), and
//! exposes a small `/health` HTTP endpoint so compose can health-check the worker. The real
//! ingestion consumer (streaming download → schema inference → ClickHouse load) lands in
//! Prompt C.

mod topology;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use lapin::{Connection, ConnectionProperties};
use shared::Config;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    let config = Config::from_env()?;
    let _telemetry = shared::telemetry::init("csvoyant-worker", &config.telemetry)?;

    // Connect to RabbitMQ and declare the durable topology up front.
    let amqp =
        Arc::new(Connection::connect(&config.amqp_url, ConnectionProperties::default()).await?);
    let channel = amqp.create_channel().await?;
    topology::declare(&channel).await?;
    info!("worker connected to rabbitmq; topology declared");

    // Health server so docker-compose can probe the worker (workers have no request surface).
    let health_addr: SocketAddr = std::env::var("WORKER_HEALTH_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8081".to_string())
        .parse()?;
    let health_app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/ready", get(ready))
        .with_state(amqp);
    let listener = tokio::net::TcpListener::bind(health_addr).await?;
    info!(addr = %health_addr, "worker health endpoint listening");

    // In Prompt A the consumer is a no-op; the process stays alive serving health checks.
    axum::serve(listener, health_app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Readiness for the worker: the RabbitMQ connection is live.
async fn ready(State(amqp): State<Arc<Connection>>) -> Result<&'static str, StatusCode> {
    if amqp.status().connected() {
        Ok("ready")
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("worker shutdown signal received");
}
