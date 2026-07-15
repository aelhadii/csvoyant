//! Job endpoint handlers.

use std::net::IpAddr;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde_json::json;
use shared::clickhouse::{escape_ident, escape_sql_string};
use shared::{ChError, INGEST_TIMESTAMP_COLUMN, JobMessage, JobStatus, Role};
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

use crate::auth::guard::AuthUser;
use crate::error::{ApiResult, AppError};
use crate::jobs::JobsState;
use crate::jobs::models::{
    CreateJobRequest, CreateJobResponse, DataQuery, JOB_COLUMNS, JobResponse, JobRow,
};
use crate::response;

type JsonValue = Json<serde_json::Value>;

const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 500;

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
    let row = load_job_for_user(&jobs.pg, id, &user).await?;
    Ok(response::data(JobResponse::from(row)))
}

/// GET /jobs/{id}/dashboard — the auto-generated dashboard config for a ready job.
pub async fn get_dashboard(
    State(jobs): State<JobsState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<JsonValue> {
    let job = load_job_for_user(&jobs.pg, id, &user).await?;
    let config: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT config FROM dashboards WHERE job_id = $1")
            .bind(job.id)
            .fetch_optional(&jobs.pg)
            .await?;
    config.map(response::data).ok_or_else(|| {
        AppError::NotFound("dashboard is not available yet (job is not ready)".to_string())
    })
}

/// GET /jobs/{id}/data — paginated / sortable / filterable rows from the job's dataset table.
pub async fn get_data(
    State(jobs): State<JobsState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<DataQuery>,
) -> ApiResult<JsonValue> {
    let job = load_job_for_user(&jobs.pg, id, &user).await?;
    if job.status != JobStatus::Ready.as_str() {
        return Err(AppError::BadRequest(format!(
            "job is not ready (status: {})",
            job.status
        )));
    }
    let table = job
        .clickhouse_table
        .clone()
        .ok_or_else(|| AppError::NotFound("no dataset table for this job".to_string()))?;

    // Only real columns may be sorted/filtered on — this is the injection guard for identifiers.
    let allowed = schema_columns(&job);

    let page = q.page.unwrap_or(1).max(1);
    let page_size = q
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = u64::from(page - 1) * u64::from(page_size);

    // Hide the synthetic TTL column from callers.
    let mut sql = format!(
        "SELECT * EXCEPT (`{ts}`) FROM `{t}`",
        ts = escape_ident(INGEST_TIMESTAMP_COLUMN),
        t = escape_ident(&table),
    );

    if let Some(filter) = q.filter.as_deref() {
        let (column, needle) = filter
            .split_once(':')
            .ok_or_else(|| AppError::BadRequest("filter must be `column:value`".to_string()))?;
        require_column(&allowed, column)?;
        sql.push_str(&format!(
            " WHERE positionCaseInsensitive(toString(`{}`), '{}') > 0",
            escape_ident(column),
            escape_sql_string(needle),
        ));
    }

    if let Some(sort) = q.sort.as_deref() {
        require_column(&allowed, sort)?;
        let dir = match q
            .order
            .as_deref()
            .unwrap_or("asc")
            .to_ascii_lowercase()
            .as_str()
        {
            "asc" => "ASC",
            "desc" => "DESC",
            other => {
                return Err(AppError::BadRequest(format!(
                    "order must be `asc` or `desc` (got `{other}`)"
                )));
            }
        };
        sql.push_str(&format!(" ORDER BY `{}` {dir}", escape_ident(sort)));
    }

    sql.push_str(&format!(
        " LIMIT {page_size} OFFSET {offset} \
         SETTINGS output_format_json_quote_64bit_integers = 0 FORMAT JSONEachRow"
    ));

    let body = jobs.ch.run(&sql).await.map_err(ch_error)?;
    let rows: Vec<serde_json::Value> = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    Ok(response::data(json!({
        "rows": rows,
        "page": page,
        "page_size": page_size,
        "total": job.row_count,
    })))
}

/// Load a job enforcing tenancy: Users see only their own, Admins see all. A job owned by
/// someone else is reported as 404 so its existence isn't leaked.
async fn load_job_for_user(pg: &PgPool, id: Uuid, user: &AuthUser) -> ApiResult<JobRow> {
    let row = sqlx::query_as::<_, JobRow>(&format!(
        "SELECT {JOB_COLUMNS} FROM ingestion_jobs WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pg)
    .await?;

    match row {
        Some(r) if r.user_id == user.user_id || user.role.satisfies(Role::Admin) => Ok(r),
        _ => Err(AppError::NotFound("job not found".to_string())),
    }
}

/// Column names from the job's stored inferred schema.
fn schema_columns(job: &JobRow) -> Vec<String> {
    job.inferred_schema
        .as_ref()
        .and_then(|s| s.get("columns"))
        .and_then(|c| c.as_array())
        .map(|cols| {
            cols.iter()
                .filter_map(|c| Some(c.get("name")?.as_str()?.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn require_column(allowed: &[String], name: &str) -> ApiResult<()> {
    if allowed.iter().any(|c| c == name) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!("unknown column: `{name}`")))
    }
}

fn ch_error(e: ChError) -> AppError {
    AppError::Internal(anyhow::anyhow!("clickhouse query failed: {e}"))
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

    // SSRF guard: resolve the host and reject private/loopback/link-local addresses so a user
    // can't make us (or ClickHouse) fetch internal services or cloud metadata. (Best-effort:
    // does not defend against DNS rebinding between this check and the actual fetch.)
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::BadRequest("URL has no host".to_string()))?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| AppError::BadRequest("URL host could not be resolved".to_string()))?;
    let mut resolved_any = false;
    for addr in addrs {
        resolved_any = true;
        if is_blocked_ip(addr.ip()) {
            return Err(AppError::BadRequest(
                "URL resolves to a disallowed (private/loopback) address".to_string(),
            ));
        }
    }
    if !resolved_any {
        return Err(AppError::BadRequest(
            "URL host could not be resolved".to_string(),
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
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

/// Whether an IP is in a range we refuse to fetch (private, loopback, link-local, etc.).
fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique-local  fc00::/7
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local    fe80::/10
                || v6.to_ipv4_mapped().is_some_and(|m| is_blocked_ip(IpAddr::V4(m)))
        }
    }
}
