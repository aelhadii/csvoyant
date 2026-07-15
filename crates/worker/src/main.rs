//! CSVoyant worker — RabbitMQ consumer + ingestion pipeline.
//!
//! Loads config, initializes telemetry, connects to Postgres / ClickHouse / RabbitMQ, declares
//! the durable topology, and consumes ingestion jobs. Each job is driven through the state
//! machine (queued → downloading → inferring → ingesting → ready|failed) using ClickHouse's
//! `url()` table function; failures are retried with backoff or dead-lettered.
//! A small `/health` endpoint lets docker-compose probe the worker.

mod clickhouse;
mod consumer;
mod dashboard;
mod error;
mod ingest;
mod repo;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use lapin::{Connection, ConnectionProperties};
use shared::Config;
use sqlx::postgres::PgPoolOptions;
use tracing::info;

use clickhouse::ChClient;
use ingest::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    let config = Config::from_env()?;
    let _telemetry = shared::telemetry::init("csvoyant-worker", &config.telemetry)?;

    // Downstreams the pipeline needs.
    let pg = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    let ch = ChClient::new(&config)?;
    let ctx = Arc::new(Context { pg, ch });

    // RabbitMQ: one connection, a channel for the consumer (which also republishes retries).
    let amqp =
        Arc::new(Connection::connect(&config.amqp_url, ConnectionProperties::default()).await?);
    let channel = amqp.create_channel().await?;
    shared::amqp::declare_topology(&channel).await?;
    info!("worker connected to rabbitmq; topology declared");

    let health_addr: SocketAddr = std::env::var("WORKER_HEALTH_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8081".to_string())
        .parse()?;
    let health_app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/ready", get(ready))
        .with_state(amqp);
    let listener = tokio::net::TcpListener::bind(health_addr).await?;
    info!(addr = %health_addr, "worker health endpoint listening");

    // Run the consumer and the health server together. If the consumer loop ever stops, exit
    // with an error so the orchestrator restarts us — otherwise the worker would keep reporting
    // healthy while silently processing nothing.
    let consumer_ctx = ctx.clone();
    tokio::select! {
        result = consumer::run(channel, consumer_ctx) => {
            tracing::error!(consumer_result = ?result, "consumer loop stopped; exiting for restart");
            anyhow::bail!("consumer loop stopped");
        }
        result = axum::serve(listener, health_app).with_graceful_shutdown(shutdown_signal()) => {
            result?;
        }
    }

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
