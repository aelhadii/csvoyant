//! Core domain vocabulary (the ubiquitous language), shared across API, worker, and tests.
//!
//! Keeping these types in one crate means every service and every test speaks the same
//! language for roles, job lifecycle, and inferred schemas.

use serde::{Deserialize, Serialize};

/// The two actors in the system. `Admin` is a strict superset of `User`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Admin,
}

impl Role {
    /// Whether this role satisfies a requirement for `required` (Admin satisfies User).
    pub fn satisfies(self, required: Role) -> bool {
        match required {
            Role::User => true,
            Role::Admin => self == Role::Admin,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Admin => "admin",
        }
    }
}

/// The lifecycle of an ingestion job. Transitions are linear until a terminal state:
/// `Queued → Downloading → Inferring → Ingesting → Ready`, or `Failed` from any stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Downloading,
    Inferring,
    Ingesting,
    Ready,
    Failed,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Downloading => "downloading",
            JobStatus::Inferring => "inferring",
            JobStatus::Ingesting => "ingesting",
            JobStatus::Ready => "ready",
            JobStatus::Failed => "failed",
        }
    }

    /// Terminal states have no outgoing transitions.
    pub fn is_terminal(self) -> bool {
        matches!(self, JobStatus::Ready | JobStatus::Failed)
    }

    /// The expected next status in the happy-path pipeline, or `None` if terminal.
    pub fn next(self) -> Option<JobStatus> {
        match self {
            JobStatus::Queued => Some(JobStatus::Downloading),
            JobStatus::Downloading => Some(JobStatus::Inferring),
            JobStatus::Inferring => Some(JobStatus::Ingesting),
            JobStatus::Ingesting => Some(JobStatus::Ready),
            JobStatus::Ready | JobStatus::Failed => None,
        }
    }

    /// Whether `self → to` is a legal transition. Any non-terminal state may fail.
    pub fn can_transition_to(self, to: JobStatus) -> bool {
        if self.is_terminal() {
            return false;
        }
        to == JobStatus::Failed || self.next() == Some(to)
    }
}

/// A column type inferred from a CSV, in widening order.
/// Inference attempts the narrowest type first and widens to `String` on any parse failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnType {
    Int64,
    Float64,
    Bool,
    Date,
    DateTime,
    String,
}

impl ColumnType {
    /// The ClickHouse type name for this inferred type (before nullability is applied).
    pub fn clickhouse_type(self) -> &'static str {
        match self {
            ColumnType::Int64 => "Int64",
            ColumnType::Float64 => "Float64",
            ColumnType::Bool => "Bool",
            ColumnType::Date => "Date",
            ColumnType::DateTime => "DateTime",
            ColumnType::String => "String",
        }
    }
}

/// One inferred column: its (sanitized) name, type, and whether empties were seen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub column_type: ColumnType,
    pub nullable: bool,
}

/// The full inferred schema for a dataset, stored on the job and used to `CREATE TABLE`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferredSchema {
    pub columns: Vec<ColumnSchema>,
}

/// The ClickHouse table name for a job, isolating each tenant's data.
pub fn dataset_table_name(user_id: uuid::Uuid, job_id: uuid::Uuid) -> String {
    format!("u{}_j{}", user_id.simple(), job_id.simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_satisfies_user_but_not_vice_versa() {
        assert!(Role::Admin.satisfies(Role::User));
        assert!(Role::Admin.satisfies(Role::Admin));
        assert!(Role::User.satisfies(Role::User));
        assert!(!Role::User.satisfies(Role::Admin));
    }

    #[test]
    fn happy_path_is_a_linear_pipeline() {
        let mut s = JobStatus::Queued;
        let expected = [
            JobStatus::Downloading,
            JobStatus::Inferring,
            JobStatus::Ingesting,
            JobStatus::Ready,
        ];
        for want in expected {
            let next = s.next().expect("non-terminal has a next state");
            assert_eq!(next, want);
            assert!(s.can_transition_to(next));
            s = next;
        }
        assert!(s.is_terminal());
        assert_eq!(s.next(), None);
    }

    #[test]
    fn any_active_stage_can_fail() {
        for s in [
            JobStatus::Queued,
            JobStatus::Downloading,
            JobStatus::Inferring,
            JobStatus::Ingesting,
        ] {
            assert!(s.can_transition_to(JobStatus::Failed));
        }
    }

    #[test]
    fn terminal_states_cannot_transition() {
        for s in [JobStatus::Ready, JobStatus::Failed] {
            assert!(!s.can_transition_to(JobStatus::Failed));
            assert!(!s.can_transition_to(JobStatus::Queued));
        }
    }

    #[test]
    fn cannot_skip_a_stage() {
        assert!(!JobStatus::Queued.can_transition_to(JobStatus::Ingesting));
        assert!(!JobStatus::Downloading.can_transition_to(JobStatus::Ready));
    }

    #[test]
    fn dataset_table_name_is_prefixed_per_tenant() {
        let user = uuid::Uuid::nil();
        let job = uuid::Uuid::nil();
        let name = dataset_table_name(user, job);
        assert!(name.starts_with("u"));
        assert!(name.contains("_j"));
        // ClickHouse identifiers: no dashes.
        assert!(!name.contains('-'));
    }
}
