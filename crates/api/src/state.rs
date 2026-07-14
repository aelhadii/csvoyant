//! Shared application state: connection handles to every downstream, plus readiness probes.

use std::sync::Arc;

use lapin::{Connection, ConnectionProperties};
use shared::Config;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing::warn;

use crate::PROBE_TIMEOUT;

/// Cloneable handle carried by every request (Axum `State`).
#[derive(Clone)]
pub struct AppState {
    pub pg: PgPool,
    pub clickhouse: clickhouse::Client,
    pub amqp: Arc<Connection>,
}

impl AppState {
    /// Establish all downstream connections. Fails fast if any cannot be reached at startup.
    pub async fn connect(config: &Config) -> anyhow::Result<Self> {
        let pg = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(PROBE_TIMEOUT)
            .connect(&config.database_url)
            .await?;

        let clickhouse = clickhouse::Client::default()
            .with_url(&config.clickhouse_url)
            .with_user(&config.clickhouse_user)
            .with_password(&config.clickhouse_password)
            .with_database(&config.clickhouse_database);

        let amqp =
            Arc::new(Connection::connect(&config.amqp_url, ConnectionProperties::default()).await?);

        Ok(Self {
            pg,
            clickhouse,
            amqp,
        })
    }

    /// Probe every dependency; returns `(name, healthy)` pairs for the `/ready` response.
    pub async fn readiness(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("postgres", self.check_postgres().await),
            ("clickhouse", self.check_clickhouse().await),
            ("rabbitmq", self.check_rabbitmq()),
        ]
    }

    async fn check_postgres(&self) -> bool {
        match tokio::time::timeout(PROBE_TIMEOUT, sqlx::query("SELECT 1").execute(&self.pg)).await {
            Ok(Ok(_)) => true,
            other => {
                warn!(?other, "postgres readiness probe failed");
                false
            }
        }
    }

    async fn check_clickhouse(&self) -> bool {
        let fut = self.clickhouse.query("SELECT 1").execute();
        match tokio::time::timeout(PROBE_TIMEOUT, fut).await {
            Ok(Ok(())) => true,
            other => {
                warn!(?other, "clickhouse readiness probe failed");
                false
            }
        }
    }

    fn check_rabbitmq(&self) -> bool {
        self.amqp.status().connected()
    }
}
