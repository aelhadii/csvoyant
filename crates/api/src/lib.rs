//! CSVoyant API library: auth, errors, and shared application state.
//!
//! The `api` binary (`main.rs`) is a thin shell over this crate; integration tests link
//! against the library so they can build routers and exercise handlers directly.

pub mod auth;
pub mod error;
pub mod state;

use std::time::Duration;

/// A short timeout applied to each readiness probe so `/ready` can't hang.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
