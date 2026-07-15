//! Extractors that map axum's built-in rejections onto [`AppError`], so that *every* response —
//! including a malformed path segment, query string, or JSON body — comes back in the uniform
//! `{ "data": …, "error": … }` envelope instead of axum's default plain-text rejection.

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Path, Query, Request};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::error::AppError;

/// `Json<T>` whose rejection is an [`AppError`].
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|e: JsonRejection| AppError::BadRequest(e.body_text()))?;
        Ok(ApiJson(value))
    }
}

/// `Query<T>` whose rejection is an [`AppError`].
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|e: QueryRejection| AppError::BadRequest(e.body_text()))?;
        Ok(ApiQuery(value))
    }
}

/// `Path<T>` whose rejection is an [`AppError`].
pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(value) = Path::<T>::from_request_parts(parts, state)
            .await
            .map_err(|e: PathRejection| AppError::BadRequest(e.body_text()))?;
        Ok(ApiPath(value))
    }
}
