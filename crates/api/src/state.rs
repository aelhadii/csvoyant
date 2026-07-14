//! Shared application state: connection handles to every downstream, plus readiness probes.

use std::sync::Arc;

use axum::extract::FromRef;
use lapin::{Connection, ConnectionProperties};
use shared::Config;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing::warn;

use crate::PROBE_TIMEOUT;
use crate::auth::{AuthState, JwtConfig};

/// Cloneable handle carried by every request (Axum `State`).
#[derive(Clone)]
pub struct AppState {
    pub pg: PgPool,
    pub clickhouse: clickhouse::Client,
    pub amqp: Arc<Connection>,
    pub auth: AuthState,
}

impl FromRef<AppState> for AuthState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

impl AppState {
    /// Establish all downstream connections. Fails fast if any cannot be reached at startup.
    pub async fn connect(config: &Config) -> anyhow::Result<Self> {
        let pg = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(PROBE_TIMEOUT)
            .connect(&config.database_url)
            .await?;

        // Apply pending migrations on startup so the schema is always current.
        sqlx::migrate!("./migrations").run(&pg).await?;
        tracing::info!("database migrations applied");

        let clickhouse = clickhouse::Client::default()
            .with_url(&config.clickhouse_url)
            .with_user(&config.clickhouse_user)
            .with_password(&config.clickhouse_password)
            .with_database(&config.clickhouse_database);

        let amqp =
            Arc::new(Connection::connect(&config.amqp_url, ConnectionProperties::default()).await?);

        let auth = AuthState {
            pg: pg.clone(),
            jwt: JwtConfig::new(config.jwt_secret.clone()),
        };

        Ok(Self {
            pg,
            clickhouse,
            amqp,
            auth,
        })
    }

    /// Probe every dependency; returns `(name, healthy)` pairs for the `/ready` response.
    /// The two async probes run concurrently so `/ready` latency is the max, not the sum.
    pub async fn readiness(&self) -> Vec<(&'static str, bool)> {
        let (pg, ch) = tokio::join!(self.check_postgres(), self.check_clickhouse());
        vec![
            ("postgres", pg),
            ("clickhouse", ch),
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
