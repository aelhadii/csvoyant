//! The ingestion pipeline: drive one job from queued to ready|failed using ClickHouse's
//! `url()` table function (DECISIONS #13) — ClickHouse fetches, parses, infers, and loads.

use serde::Deserialize;
use shared::{DATA_RETENTION_DAYS, INGEST_TIMEOUT_SECS, JobMessage, JobStatus, dataset_table_name};
use sqlx::PgPool;
use tracing::{Instrument, info, info_span};

use crate::clickhouse::ChClient;
use crate::error::{IngestError, db_retryable};
use crate::repo;

/// Everything a job needs: the app DB and the ClickHouse client.
pub struct Context {
    pub pg: PgPool,
    pub ch: ChClient,
}

/// One column of ClickHouse's inferred schema (`DESCRIBE … FORMAT JSONEachRow`).
#[derive(Debug, Deserialize)]
struct DescribedColumn {
    name: String,
    r#type: String,
}

/// Run the full pipeline for one job. Each stage is its own span (job/user id as attributes).
pub async fn run_job(ctx: &Context, msg: &JobMessage) -> Result<(), IngestError> {
    let table = dataset_table_name(msg.user_id, msg.job_id);
    let src = escape_sql_string(&msg.source_url);

    // Idempotency (at-least-once delivery): a duplicate for an already-finished job is a no-op.
    if repo::is_ready(&ctx.pg, msg.job_id)
        .await
        .map_err(db_retryable)?
    {
        info!(job_id = %msg.job_id, "job already ready; skipping duplicate delivery");
        return Ok(());
    }

    // ClickHouse does the fetch during DESCRIBE/INSERT; reflect that as "downloading".
    repo::set_status(&ctx.pg, msg.job_id, JobStatus::Downloading)
        .await
        .map_err(db_retryable)?;

    // Inferring — ask ClickHouse to infer the schema from the source.
    repo::set_status(&ctx.pg, msg.job_id, JobStatus::Inferring)
        .await
        .map_err(db_retryable)?;
    let columns = describe_schema(ctx, &src)
        .instrument(info_span!("infer_schema"))
        .await?;
    if columns.is_empty() {
        return Err(IngestError::Permanent(
            "no columns were detected in the CSV".to_string(),
        ));
    }

    // Ingesting — create the per-job table (with TTL) and load via INSERT … SELECT url().
    repo::set_status(&ctx.pg, msg.job_id, JobStatus::Ingesting)
        .await
        .map_err(db_retryable)?;
    // Drop any prior/partial table first so a redelivery re-ingests cleanly (no duplicate rows).
    ctx.ch
        .run(&format!("DROP TABLE IF EXISTS `{}`", escape_ident(&table)))
        .await?;
    ctx.ch
        .run(&build_create_table(&table, &columns))
        .instrument(info_span!("create_table"))
        .await?;
    ctx.ch
        .run(&build_insert(&table, &columns, &src))
        .instrument(info_span!("insert"))
        .await?;

    // Ready — record the row count and inferred schema.
    let row_count = count_rows(ctx, &table)
        .instrument(info_span!("count_rows"))
        .await?;
    repo::mark_ready(
        &ctx.pg,
        msg.job_id,
        &table,
        row_count,
        schema_to_json(&columns),
    )
    .await
    .map_err(db_retryable)?;
    info!(job_id = %msg.job_id, rows = row_count, table = %table, "ingestion complete");
    Ok(())
}

async fn describe_schema(ctx: &Context, src: &str) -> Result<Vec<DescribedColumn>, IngestError> {
    // No explicit format: ClickHouse auto-detects format (CSV/TSV/Parquet/JSON), compression
    // (.xz/.gz/.zst), and the header row from the URL. The outer FORMAT is the DESCRIBE output.
    let sql = format!(
        "DESCRIBE TABLE url('{src}') \
         SETTINGS max_execution_time = {INGEST_TIMEOUT_SECS} FORMAT JSONEachRow"
    );
    parse_describe(&ctx.ch.run(&sql).await?)
}

/// Parse `DESCRIBE … FORMAT JSONEachRow` output (one JSON object per line).
fn parse_describe(body: &str) -> Result<Vec<DescribedColumn>, IngestError> {
    let mut columns = Vec::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let col: DescribedColumn = serde_json::from_str(line)
            .map_err(|e| IngestError::Permanent(format!("could not read inferred schema: {e}")))?;
        columns.push(col);
    }
    Ok(columns)
}

async fn count_rows(ctx: &Context, table: &str) -> Result<i64, IngestError> {
    let sql = format!(
        "SELECT count() FROM `{}` FORMAT TabSeparated",
        escape_ident(table)
    );
    Ok(ctx.ch.run(&sql).await?.trim().parse::<i64>().unwrap_or(0))
}

fn build_create_table(table: &str, columns: &[DescribedColumn]) -> String {
    let cols = columns
        .iter()
        .map(|c| format!("`{}` {}", escape_ident(&c.name), c.r#type))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE TABLE IF NOT EXISTS `{table}` ({cols}, `_ingested_at` DateTime DEFAULT now()) \
         ENGINE = MergeTree ORDER BY tuple() \
         TTL `_ingested_at` + INTERVAL {DATA_RETENTION_DAYS} DAY",
        table = escape_ident(table),
    )
}

fn build_insert(table: &str, columns: &[DescribedColumn], src: &str) -> String {
    let col_list = columns
        .iter()
        .map(|c| format!("`{}`", escape_ident(&c.name)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO `{table}` ({col_list}) SELECT * FROM url('{src}') \
         SETTINGS max_execution_time = {INGEST_TIMEOUT_SECS}",
        table = escape_ident(table),
    )
}

fn schema_to_json(columns: &[DescribedColumn]) -> serde_json::Value {
    serde_json::json!({
        "columns": columns
            .iter()
            .map(|c| serde_json::json!({ "name": c.name, "type": c.r#type }))
            .collect::<Vec<_>>()
    })
}

/// Escape a value going inside a single-quoted SQL string literal (guards the source URL).
fn escape_sql_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Escape a backtick-quoted identifier.
fn escape_ident(s: &str) -> String {
    s.replace('`', "``")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols() -> Vec<DescribedColumn> {
        vec![
            DescribedColumn {
                name: "id".into(),
                r#type: "Int64".into(),
            },
            DescribedColumn {
                name: "price".into(),
                r#type: "Nullable(Float64)".into(),
            },
        ]
    }

    #[test]
    fn parses_describe_json_lines() {
        let body =
            "{\"name\":\"id\",\"type\":\"Int64\"}\n{\"name\":\"name\",\"type\":\"String\"}\n";
        let parsed = parse_describe(body).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "id");
        assert_eq!(parsed[1].r#type, "String");
    }

    #[test]
    fn malformed_describe_output_is_a_permanent_error() {
        let err = parse_describe("this is not json").unwrap_err();
        assert!(!err.is_retryable());
    }

    #[test]
    fn create_table_has_ttl_and_ingest_timestamp() {
        let ddl = build_create_table("u1_j2", &cols());
        assert!(ddl.contains("`_ingested_at` DateTime DEFAULT now()"));
        assert!(ddl.contains(&format!("INTERVAL {DATA_RETENTION_DAYS} DAY")));
        assert!(ddl.contains("`id` Int64"));
        assert!(ddl.contains("`price` Nullable(Float64)"));
    }

    #[test]
    fn insert_uses_url_function_and_column_list() {
        let sql = build_insert("u1_j2", &cols(), "https://example.com/f.csv");
        assert!(sql.contains("INSERT INTO `u1_j2` (`id`, `price`)"));
        assert!(sql.contains("url('https://example.com/f.csv')"));
        assert!(sql.contains(&format!("max_execution_time = {INGEST_TIMEOUT_SECS}")));
    }

    #[test]
    fn source_url_is_escaped_against_sql_injection() {
        let sql = build_insert("t", &cols(), &escape_sql_string("x'); DROP TABLE users;--"));
        // The injected quote is backslash-escaped, so it can't close the string literal.
        assert!(sql.contains("x\\'); DROP TABLE users;--"));
    }
}
