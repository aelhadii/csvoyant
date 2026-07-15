//! Ingestion errors, classified by whether a retry could plausibly succeed.

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// Won't succeed on retry: bad URL, non-CSV content, parse failure, oversize.
    #[error("{0}")]
    Permanent(String),
    /// Might succeed on retry: ClickHouse unreachable, transient 5xx, timeout.
    #[error("{0}")]
    Retryable(String),
}

impl IngestError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, IngestError::Retryable(_))
    }

    /// The user-facing failure reason stored on the job.
    pub fn message(&self) -> &str {
        match self {
            IngestError::Permanent(m) | IngestError::Retryable(m) => m,
        }
    }
}

/// A database error while driving the job row is always worth retrying.
pub fn db_retryable(e: sqlx::Error) -> IngestError {
    IngestError::Retryable(format!("database error: {e}"))
}
