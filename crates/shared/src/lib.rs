//! Shared types and setup for the CSVoyant platform: configuration, the domain vocabulary
//! (ubiquitous language), and telemetry initialization used by both the API and the worker.

pub mod config;
pub mod domain;
pub mod telemetry;

pub use config::{Config, TelemetryConfig};
pub use domain::{ColumnSchema, ColumnType, InferredSchema, JobStatus, Role, dataset_table_name};

/// The AMQP queue ingestion jobs are published to and consumed from.
pub const INGESTION_QUEUE: &str = "ingestion.jobs";
/// The dead-letter exchange failed/retried messages are routed to.
pub const DEAD_LETTER_EXCHANGE: &str = "ingestion.dlx";
/// The dead-letter queue bound to [`DEAD_LETTER_EXCHANGE`].
pub const DEAD_LETTER_QUEUE: &str = "ingestion.jobs.dead";
