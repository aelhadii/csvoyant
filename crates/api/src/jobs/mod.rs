//! Ingestion job submission + status endpoints.
//!
//! `POST /jobs` validates a CSV URL, persists a queued job, and publishes it to RabbitMQ.
//! `GET /jobs` and `GET /jobs/{id}` return status with per-tenant isolation (Users see only
//! their own jobs; Admins see all).

pub mod handlers;
pub mod models;
pub mod publisher;
pub mod relay;

use std::sync::Arc;

use axum::Router;
use axum::extract::FromRef;
use axum::routing::{get, post};
use shared::ChHttp;
use sqlx::PgPool;
use tokio::sync::Notify;

use crate::auth::AuthState;

/// State the job endpoints need. `POST /jobs` writes the job + an outbox row in one transaction
/// (so the enqueue can't be lost), then nudges the relay to publish promptly. `ch` serves the
/// dashboard/data reads straight from ClickHouse.
#[derive(Clone)]
pub struct JobsState {
    pub pg: PgPool,
    pub relay_notify: Arc<Notify>,
    pub ch: ChHttp,
}

pub fn jobs_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    JobsState: FromRef<S>,
    AuthState: FromRef<S>,
{
    Router::new()
        .route("/jobs", post(handlers::create_job).get(handlers::list_jobs))
        .route("/jobs/{id}", get(handlers::get_job))
        .route("/jobs/{id}/dashboard", get(handlers::get_dashboard))
        .route("/jobs/{id}/data", get(handlers::get_data))
}
