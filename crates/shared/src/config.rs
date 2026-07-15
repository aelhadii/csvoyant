//! Environment-driven configuration shared by every binary.
//!
//! Config comes from the process environment only (see `.env.example`) so the stack can move
//! off docker-compose without code changes.

use std::env;

/// Read a required env var, returning a descriptive error if it is missing.
fn required(key: &str) -> anyhow::Result<String> {
    env::var(key).map_err(|_| anyhow::anyhow!("missing required env var: {key}"))
}

/// Read an optional env var with a fallback default.
fn optional(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// OpenTelemetry / Axiom export configuration.
#[derive(Clone, Debug)]
pub struct TelemetryConfig {
    /// OTLP/HTTP base endpoint (e.g. `https://api.axiom.co`). If empty, only stdout logging is used.
    pub otlp_endpoint: String,
    /// Axiom API token (sent as `Authorization: Bearer`). Optional for a bare OTLP collector.
    pub axiom_token: String,
    /// Axiom dataset (sent as `X-Axiom-Dataset`). Optional for a bare OTLP collector.
    pub axiom_dataset: String,
}

impl TelemetryConfig {
    pub fn from_env() -> Self {
        Self {
            otlp_endpoint: optional("OTLP_ENDPOINT", ""),
            axiom_token: optional("AXIOM_TOKEN", ""),
            axiom_dataset: optional("AXIOM_DATASET", ""),
        }
    }

    /// Telemetry export is only enabled when an OTLP endpoint is configured.
    pub fn export_enabled(&self) -> bool {
        !self.otlp_endpoint.is_empty()
    }
}

/// Full application configuration, loaded once at startup.
#[derive(Clone, Debug)]
pub struct Config {
    /// PostgreSQL connection string (application state).
    pub database_url: String,
    /// ClickHouse HTTP URL (analytics store).
    pub clickhouse_url: String,
    /// ClickHouse user / password / database.
    pub clickhouse_user: String,
    pub clickhouse_password: String,
    pub clickhouse_database: String,
    /// RabbitMQ AMQP URL.
    pub amqp_url: String,
    /// Secret used to sign JWTs.
    pub jwt_secret: String,
    /// Host:port the HTTP server binds to.
    pub bind_addr: String,
    /// Browser origin allowed to call the API with credentials (the frontend).
    pub cors_allowed_origin: String,
    /// Telemetry export config.
    pub telemetry: TelemetryConfig,
}

impl Config {
    /// Load configuration from the environment. Call [`dotenvy`] beforehand in `main` if desired.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            clickhouse_url: optional("CLICKHOUSE_URL", "http://localhost:8123"),
            clickhouse_user: optional("CLICKHOUSE_USER", "default"),
            clickhouse_password: optional("CLICKHOUSE_PASSWORD", ""),
            clickhouse_database: optional("CLICKHOUSE_DATABASE", "default"),
            amqp_url: required("AMQP_URL")?,
            jwt_secret: optional("JWT_SECRET", "dev-insecure-change-me"),
            bind_addr: optional("BIND_ADDR", "0.0.0.0:8080"),
            cors_allowed_origin: optional("CORS_ALLOWED_ORIGIN", "http://localhost:3000"),
            telemetry: TelemetryConfig::from_env(),
        })
    }
}
