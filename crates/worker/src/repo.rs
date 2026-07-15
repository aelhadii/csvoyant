//! Postgres updates to the `ingestion_jobs` row as it moves through the state machine.

use serde_json::Value;
use shared::JobStatus;
use sqlx::PgPool;
use uuid::Uuid;

/// Whether the job is already `ready` — used to skip duplicate (at-least-once) deliveries.
pub async fn is_ready(pg: &PgPool, job_id: Uuid) -> Result<bool, sqlx::Error> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM ingestion_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_optional(pg)
            .await?;
    Ok(status.as_deref() == Some("ready"))
}

/// Move a job to a non-terminal status (downloading / inferring / ingesting).
pub async fn set_status(pg: &PgPool, job_id: Uuid, status: JobStatus) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE ingestion_jobs SET status = $2 WHERE id = $1")
        .bind(job_id)
        .bind(status.as_str())
        .execute(pg)
        .await?;
    Ok(())
}

/// Mark a job ready: record the ClickHouse table, row count, inferred schema, finish time.
pub async fn mark_ready(
    pg: &PgPool,
    job_id: Uuid,
    table: &str,
    row_count: i64,
    schema: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ingestion_jobs \
         SET status = 'ready', clickhouse_table = $2, row_count = $3, inferred_schema = $4, \
             error = NULL, finished_at = now() \
         WHERE id = $1",
    )
    .bind(job_id)
    .bind(table)
    .bind(row_count)
    .bind(schema)
    .execute(pg)
    .await?;
    Ok(())
}

/// Mark a job failed with a user-facing reason.
pub async fn mark_failed(pg: &PgPool, job_id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ingestion_jobs SET status = 'failed', error = $2, finished_at = now() WHERE id = $1",
    )
    .bind(job_id)
    .bind(error)
    .execute(pg)
    .await?;
    Ok(())
}

/// Record that another processing attempt was made; returns the new attempt count.
pub async fn increment_attempts(pg: &PgPool, job_id: Uuid) -> Result<i32, sqlx::Error> {
    let attempts: i32 = sqlx::query_scalar(
        "UPDATE ingestion_jobs SET attempts = attempts + 1 WHERE id = $1 RETURNING attempts",
    )
    .bind(job_id)
    .fetch_one(pg)
    .await?;
    Ok(attempts)
}
