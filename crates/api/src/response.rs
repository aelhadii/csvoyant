//! The uniform API response envelope.
//!
//! Every endpoint returns the same top-level shape: `{ "data": <payload>, "error": <null|obj> }`.
//! Success responses use [`data`] (error is null); failures are produced by
//! [`crate::error::AppError`] (data is null). This lets clients always read the same two fields.

use axum::Json;
use serde::Serialize;
use serde_json::{Value, json};

/// Wrap a successful payload as `{ "data": payload, "error": null }`.
pub fn data<T: Serialize>(payload: T) -> Json<Value> {
    Json(json!({ "data": payload, "error": null }))
}
