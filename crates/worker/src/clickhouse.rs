//! A thin ClickHouse HTTP client. Every ingestion operation is a single SQL statement, so this
//! wraps "POST SQL, get text back" with error classification.

use std::time::Duration;

use shared::{Config, INGEST_TIMEOUT_SECS};

use crate::error::IngestError;

#[derive(Clone)]
pub struct ChClient {
    http: reqwest::Client,
    base: String,
    user: String,
    password: String,
    pub database: String,
}

impl ChClient {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        // Allow the statement timeout (max_execution_time) to fire before the HTTP client does.
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(INGEST_TIMEOUT_SECS + 30))
            .build()?;
        Ok(Self {
            http,
            base: config.clickhouse_url.trim_end_matches('/').to_string(),
            user: config.clickhouse_user.clone(),
            password: config.clickhouse_password.clone(),
            database: config.clickhouse_database.clone(),
        })
    }

    /// Execute a statement and return the response body.
    ///
    /// Classification note: ClickHouse returns **HTTP 500 for query exceptions too** (bad URL,
    /// unparseable data, type mismatch), so status code alone can't tell transient from
    /// permanent. A failure to *reach* ClickHouse is [`IngestError::Retryable`]; an error
    /// *response* is classified by content — only a small set of clearly-transient signatures
    /// (overload, memory, connection reset) retries; everything else is [`IngestError::Permanent`].
    pub async fn run(&self, sql: &str) -> Result<String, IngestError> {
        let response = self
            .http
            .post(&self.base)
            .basic_auth(&self.user, Some(&self.password))
            .query(&[("database", self.database.as_str())])
            .body(sql.to_string())
            .send()
            .await
            .map_err(|e| IngestError::Retryable(format!("clickhouse unreachable: {e}")))?;

        let is_success = response.status().is_success();
        let body = response.text().await.unwrap_or_default();
        if is_success {
            Ok(body)
        } else if is_transient(&body) {
            Err(IngestError::Retryable(clean_error(&body)))
        } else {
            Err(IngestError::Permanent(clean_error(&body)))
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
    use super::clean_error;

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
}
