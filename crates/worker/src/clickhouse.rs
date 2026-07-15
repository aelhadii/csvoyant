//! Worker-side ClickHouse access: wraps the shared HTTP client and classifies failures into
//! retryable vs permanent ingestion errors with user-facing messages.

use std::time::Duration;

use shared::{ChError, ChHttp, Config, INGEST_TIMEOUT_SECS};

use crate::error::IngestError;

#[derive(Clone)]
pub struct ChClient {
    http: ChHttp,
}

impl ChClient {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        // Let the SQL's max_execution_time fire before the HTTP client's timeout does.
        let http = ChHttp::new(config, Duration::from_secs(INGEST_TIMEOUT_SECS + 30))?;
        Ok(Self { http })
    }

    /// Execute a statement and return the response body.
    ///
    /// Classification note: ClickHouse returns **HTTP 500 for query exceptions too** (bad URL,
    /// unparseable data, type mismatch), so status alone can't separate transient from
    /// permanent. Failing to *reach* ClickHouse is retryable; an error *response* is classified
    /// by content — only clearly-transient signatures retry, everything else is permanent.
    pub async fn run(&self, sql: &str) -> Result<String, IngestError> {
        match self.http.run(sql).await {
            Ok(body) => Ok(body),
            Err(ChError::Unreachable(e)) => Err(IngestError::Retryable(format!(
                "clickhouse unreachable: {e}"
            ))),
            Err(ChError::Query(body)) if is_transient(&body) => {
                Err(IngestError::Retryable(clean_error(&body)))
            }
            Err(ChError::Query(body)) => Err(IngestError::Permanent(clean_error(&body))),
        }
    }
}

/// Signatures of genuinely transient ClickHouse failures worth retrying (everything else that
/// comes back as an error response is treated as a permanent data/URL problem).
fn is_transient(body: &str) -> bool {
    let l = body.to_ascii_lowercase();
    l.contains("too_many_simultaneous_queries")
        || l.contains("memory_limit")
        || l.contains("service unavailable")
        || l.contains("connection reset")
        || l.contains("no free connection")
        || l.contains("temporarily unavailable")
}

/// Turn a verbose ClickHouse error body into a short, user-facing reason.
fn clean_error(body: &str) -> String {
    let body = body.trim();
    let lower = body.to_ascii_lowercase();
    if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("max_execution_time")
    {
        return "ingestion timed out (file too large or source too slow)".to_string();
    }
    if lower.contains("cannot be detected") || lower.contains("format cannot be detected") {
        return "could not detect the file format (is it a supported data file?)".to_string();
    }
    if lower.contains("cannot parse")
        || lower.contains("cannot extract")
        || lower.contains("is not like")
    {
        return "the file could not be parsed (unexpected content for its format)".to_string();
    }
    if lower.contains("not found") || lower.contains("404") {
        return "the source URL could not be fetched (404)".to_string();
    }
    // Otherwise surface ClickHouse's first line, trimmed to something readable.
    let first_line = body.lines().next().unwrap_or("ingestion failed");
    let concise = first_line.trim_start_matches("Code:").trim();
    let truncated: String = concise.chars().take(200).collect();
    if truncated.is_empty() {
        "ingestion failed".to_string()
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::{clean_error, is_transient};

    #[test]
    fn timeouts_map_to_a_friendly_reason() {
        assert!(
            clean_error("Code: 159. DB::Exception: Timeout exceeded: max_execution_time")
                .contains("timed out")
        );
    }

    #[test]
    fn parse_errors_map_to_a_friendly_reason() {
        assert!(
            clean_error("Code: 27. DB::Exception: Cannot parse input")
                .contains("could not be parsed")
        );
    }

    #[test]
    fn undetectable_format_maps_to_a_friendly_reason() {
        let msg = clean_error(
            "Code: 715. DB::Exception: The data format cannot be detected by the contents",
        );
        assert!(msg.contains("could not detect the file format"));
    }

    #[test]
    fn unknown_errors_are_truncated_but_present() {
        let msg = clean_error("Code: 999. DB::Exception: something unusual happened");
        assert!(!msg.is_empty());
        assert!(msg.len() <= 200);
    }

    #[test]
    fn data_errors_are_not_transient_but_overload_is() {
        assert!(!is_transient(
            "Code: 715. The data format cannot be detected"
        ));
        assert!(is_transient("Code: 202. TOO_MANY_SIMULTANEOUS_QUERIES"));
    }
}
