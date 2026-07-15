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

/// A job row as stored in Postgres.
#[derive(Debug, sqlx::FromRow)]
pub struct JobRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source_url: String,
    pub status: String,
    pub error: Option<String>,
    pub row_count: Option<i64>,
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
pub const JOB_COLUMNS: &str =
    "id, user_id, source_url, status, error, row_count, created_at, finished_at";
