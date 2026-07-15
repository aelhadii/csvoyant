//! Job endpoint handlers.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use shared::{JobMessage, JobStatus, Role};
use uuid::Uuid;
use validator::Validate;

use crate::auth::guard::AuthUser;
use crate::error::{ApiResult, AppError};
use crate::jobs::JobsState;
use crate::jobs::models::{CreateJobRequest, CreateJobResponse, JOB_COLUMNS, JobResponse, JobRow};
use crate::response;

type JsonValue = Json<serde_json::Value>;

/// POST /jobs — validate the CSV URL, persist a queued job, and enqueue it.
pub async fn create_job(
    State(jobs): State<JobsState>,
    user: AuthUser,
    Json(req): Json<CreateJobRequest>,
) -> ApiResult<(StatusCode, JsonValue)> {
    req.validate()?;
    validate_source_url(&req.url).await?;

    // Transactional outbox: persist the job AND the message to publish in one transaction, so a
    // crash between the commit and the RabbitMQ publish can't lose the enqueue. The relay
    // publishes the outbox row asynchronously (at-least-once).
    let mut tx = jobs.pg.begin().await?;
    let job_id: Uuid = sqlx::query_scalar(
        "INSERT INTO ingestion_jobs (user_id, source_url) VALUES ($1, $2) RETURNING id",
    )
    .bind(user.user_id)
    .bind(&req.url)
    .fetch_one(&mut *tx)
    .await?;

    let message = JobMessage {
        job_id,
        user_id: user.user_id,
        source_url: req.url.clone(),
        attempt: 0,
    };
    let payload = serde_json::to_value(&message).expect("JobMessage serializes");
    sqlx::query("INSERT INTO outbox (queue, payload) VALUES ($1, $2)")
        .bind(shared::INGESTION_QUEUE)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    // Nudge the relay to publish now instead of on its next poll tick (best-effort).
    jobs.relay_notify.notify_one();

    Ok((
        StatusCode::ACCEPTED,
        response::data(CreateJobResponse {
            job_id,
            status: JobStatus::Queued,
        }),
    ))
}

/// GET /jobs — the caller's jobs (Admins see everyone's), newest first.
pub async fn list_jobs(State(jobs): State<JobsState>, user: AuthUser) -> ApiResult<JsonValue> {
    let rows = if user.role.satisfies(Role::Admin) {
        sqlx::query_as::<_, JobRow>(&format!(
            "SELECT {JOB_COLUMNS} FROM ingestion_jobs ORDER BY created_at DESC"
        ))
        .fetch_all(&jobs.pg)
        .await?
    } else {
        sqlx::query_as::<_, JobRow>(&format!(
            "SELECT {JOB_COLUMNS} FROM ingestion_jobs WHERE user_id = $1 ORDER BY created_at DESC"
        ))
        .bind(user.user_id)
        .fetch_all(&jobs.pg)
        .await?
    };
    let jobs: Vec<JobResponse> = rows.into_iter().map(JobResponse::from).collect();
    Ok(response::data(jobs))
}

/// GET /jobs/{id} — one job's status. Cross-tenant access is reported as 404 (existence hidden).
pub async fn get_job(
    State(jobs): State<JobsState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<JsonValue> {
    let row = sqlx::query_as::<_, JobRow>(&format!(
        "SELECT {JOB_COLUMNS} FROM ingestion_jobs WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&jobs.pg)
    .await?;

    let row = match row {
        Some(r) if r.user_id == user.user_id || user.role.satisfies(Role::Admin) => r,
        _ => return Err(AppError::NotFound("job not found".into())),
    };
    Ok(response::data(JobResponse::from(row)))
}

/// Validate a submitted data-file URL: scheme, reachability, size, and (if advertised) that it
/// isn't a web page. Fast pre-flight only; ClickHouse performs the authoritative format handling.
async fn validate_source_url(raw: &str) -> ApiResult<()> {
    let parsed =
        url::Url::parse(raw).map_err(|_| AppError::BadRequest("invalid URL".to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(
            "URL scheme must be http or https".to_string(),
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(e.into()))?;

    // Prefer HEAD; some servers reject it, so fall back to a 1-byte ranged GET.
    let response = match client.head(parsed.clone()).send().await {
        Ok(r) => r,
        Err(_) => client
            .get(parsed)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|_| AppError::BadRequest("URL is not reachable".to_string()))?,
    };

    let status = response.status();
    if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(AppError::BadRequest(format!(
            "URL returned HTTP {}",
            status.as_u16()
        )));
    }

    // Best-effort size guard (DECISIONS #2): reject up front if Content-Length exceeds the cap.
    if let Some(len) = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        && len > shared::MAX_FILE_SIZE_BYTES
    {
        return Err(AppError::BadRequest(format!(
            "file is too large ({len} bytes; limit is {} bytes)",
            shared::MAX_FILE_SIZE_BYTES
        )));
    }

    // ClickHouse auto-detects the actual format (CSV/TSV/Parquet/JSON + compression), so we only
    // reject obvious web pages here — a paste of an HTML page is the common mistake.
    if let Some(content_type) = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        let ct = content_type.to_ascii_lowercase();
        if ct.contains("html") || ct.contains("application/xml") || ct.contains("text/xml") {
            return Err(AppError::BadRequest(format!(
                "URL looks like a web page, not a data file (content-type: {content_type})"
            )));
        }
    }

    Ok(())
}
