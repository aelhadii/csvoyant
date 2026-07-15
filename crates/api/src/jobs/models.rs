//! Job DTOs and the persisted job row.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::JobStatus;
use uuid::Uuid;
use validator::Validate;

/// Submit-a-CSV-URL payload.
#[derive(Debug, Deserialize, Validate)]
pub struct CreateJobRequest {
    #[validate(url(message = "must be a valid URL"))]
    pub url: String,
}

/// Response to a successful submission.
#[derive(Debug, Serialize)]
pub struct CreateJobResponse {
    pub job_id: Uuid,
    pub status: JobStatus,
}

/// A job row as stored in Postgres. `clickhouse_table` / `inferred_schema` are internal and are
/// not exposed in [`JobResponse`]; the read endpoints use them to query the dataset.
#[derive(Debug, sqlx::FromRow)]
pub struct JobRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source_url: String,
    pub status: String,
    pub error: Option<String>,
    pub row_count: Option<i64>,
    pub clickhouse_table: Option<String>,
    pub inferred_schema: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Public view of a job.
#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub id: Uuid,
    pub source_url: String,
    pub status: JobStatus,
    pub error: Option<String>,
    pub row_count: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl From<JobRow> for JobResponse {
    fn from(r: JobRow) -> Self {
        let status = JobStatus::parse(&r.status).unwrap_or(JobStatus::Failed);
        JobResponse {
            id: r.id,
            source_url: r.source_url,
            status,
            error: r.error,
            row_count: r.row_count,
            created_at: r.created_at,
            finished_at: r.finished_at,
        }
    }
}

/// Columns selected for a [`JobRow`], kept in one place so every query stays consistent.
pub const JOB_COLUMNS: &str = "id, user_id, source_url, status, error, row_count, \
     clickhouse_table, inferred_schema, created_at, finished_at";

/// Query parameters for the paginated data endpoint.
#[derive(Debug, Deserialize)]
pub struct DataQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    /// Column to sort by (must be a real column of the dataset).
    pub sort: Option<String>,
    /// `asc` (default) or `desc`.
    pub order: Option<String>,
    /// `column:substring` — case-insensitive contains match.
    pub filter: Option<String>,
}
