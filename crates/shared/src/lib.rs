//! Shared types and setup for the CSVoyant platform: configuration, the domain vocabulary
//! (ubiquitous language), and telemetry initialization used by both the API and the worker.

pub mod amqp;
pub mod clickhouse;
pub mod config;
pub mod domain;
pub mod telemetry;

pub use amqp::JobMessage;
pub use clickhouse::{ChError, ChHttp};
pub use config::{Config, TelemetryConfig};
pub use domain::{ColumnSchema, ColumnType, InferredSchema, JobStatus, Role, dataset_table_name};

/// The AMQP queue ingestion jobs are published to and consumed from.
pub const INGESTION_QUEUE: &str = "ingestion.jobs";
/// The dead-letter exchange failed/retried messages are routed to.
pub const DEAD_LETTER_EXCHANGE: &str = "ingestion.dlx";
/// The dead-letter queue bound to [`DEAD_LETTER_EXCHANGE`].
pub const DEAD_LETTER_QUEUE: &str = "ingestion.jobs.dead";

/// Best-effort CSV size ceiling, checked against the URL's `Content-Length` at submit time
/// (DECISIONS #2: 500 MB). ClickHouse streams the actual fetch.
pub const MAX_FILE_SIZE_BYTES: u64 = 500 * 1024 * 1024;
/// Ingestion timeout, applied as `max_execution_time` on the ClickHouse `url()` query (120 s).
pub const INGEST_TIMEOUT_SECS: u64 = 120;
/// Maximum processing attempts before a job is dead-lettered.
pub const MAX_ATTEMPTS: u32 = 3;
/// Days ingested data is retained in ClickHouse before TTL expiry (DECISIONS: 7 days).
pub const DATA_RETENTION_DAYS: u32 = 7;
/// Synthetic column added to every dataset table to carry the ingestion time + TTL. Namespaced
/// so it won't collide with a user's CSV column; read endpoints should exclude it.
pub const INGEST_TIMESTAMP_COLUMN: &str = "_csvoyant_ingested_at";
