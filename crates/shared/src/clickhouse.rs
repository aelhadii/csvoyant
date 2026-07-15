//! A minimal ClickHouse HTTP client shared by the API (dashboard/data reads) and the worker
//! (ingestion). Every operation is "POST some SQL, get the response body back", which is all
//! we need since ClickHouse itself fetches source files via `url()` and we render results with
//! `FORMAT JSONEachRow`.

use std::time::Duration;

use crate::Config;

/// A ClickHouse failure, split by whether we ever reached the server.
#[derive(Debug, thiserror::Error)]
pub enum ChError {
    /// Could not reach ClickHouse at all (DNS/TCP/TLS/timeout) — worth retrying.
    #[error("clickhouse unreachable: {0}")]
    Unreachable(String),
    /// ClickHouse answered with an error. The body is its raw exception text; callers classify
    /// it (ClickHouse returns HTTP 500 for data errors too, so status can't be trusted).
    #[error("{0}")]
    Query(String),
}

#[derive(Clone)]
pub struct ChHttp {
    http: reqwest::Client,
    base: String,
    user: String,
    password: String,
    database: String,
}

impl ChHttp {
    /// Build a client. `timeout` bounds the whole HTTP request (set it above any
    /// `max_execution_time` you put on the SQL so ClickHouse's own limit fires first).
    pub fn new(config: &Config, timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder().timeout(timeout).build()?,
            base: config.clickhouse_url.trim_end_matches('/').to_string(),
            user: config.clickhouse_user.clone(),
            password: config.clickhouse_password.clone(),
            database: config.clickhouse_database.clone(),
        })
    }

    pub fn database(&self) -> &str {
        &self.database
    }

    /// Execute a statement and return the response body.
    pub async fn run(&self, sql: &str) -> Result<String, ChError> {
        let response = self
            .http
            .post(&self.base)
            .basic_auth(&self.user, Some(&self.password))
            .query(&[("database", self.database.as_str())])
            .body(sql.to_string())
            .send()
            .await
            .map_err(|e| ChError::Unreachable(e.to_string()))?;

        let is_success = response.status().is_success();
        let body = response.text().await.unwrap_or_default();
        if is_success {
            Ok(body)
        } else {
            Err(ChError::Query(body))
        }
    }
}

/// Parse a `FORMAT JSONEachRow` response body: one JSON object per line, blanks skipped.
pub fn parse_json_lines(body: &str) -> Vec<serde_json::Value> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Escape a backtick-quoted ClickHouse identifier.
pub fn escape_ident(s: &str) -> String {
    s.replace('`', "``")
}

/// Escape a value going inside a single-quoted SQL string literal.
pub fn escape_sql_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_escape_backticks() {
        assert_eq!(escape_ident("we`ird"), "we``ird");
    }

    #[test]
    fn string_literals_escape_quotes_and_backslashes() {
        assert_eq!(escape_sql_string("a'b"), "a\\'b");
        assert_eq!(escape_sql_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn json_each_row_parsing_skips_blanks_and_bad_lines() {
        let rows = parse_json_lines("{\"a\":1}\n\n{\"a\":2}\nnot-json\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["a"], 1);
        assert_eq!(rows[1]["a"], 2);
    }
}
