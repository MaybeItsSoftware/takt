use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Anything a request can fail with.
///
/// One type, because every failure ends the same way: a status and an
/// `{"error": "..."}` body. Clients branch on the status alone (401 means
/// sign in again, 400 means a bug in the client or a bad form field, 5xx
/// means retry later), so the
/// variants are cut along those lines rather than along where they came from.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("missing, expired or unknown access token")]
    Unauthorized,
    /// Signed in, but not allowed: not a member of the shared list, or not
    /// its owner.
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    /// An invite that was good once: used or expired.
    #[error("{0}")]
    Gone(String),
    #[error("{0}")]
    TooMany(String),
    /// Hosted sync without a subscription (`billing.rs`). The body also
    /// carries `"subscription": "required"`, which the apps look for.
    #[error("{0}")]
    PaymentRequired(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("internal error: {0}")]
    Internal(String),
    /// Logged in full, reported vaguely: a database message can carry row
    /// contents, and the client can do nothing with it but retry.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Gone(_) => StatusCode::GONE,
            AppError::TooMany(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::PaymentRequired(_) => StatusCode::PAYMENT_REQUIRED,
            AppError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = match &self {
            AppError::Database(error) => {
                tracing::error!(%error, "request failed in the database");
                "internal error".to_owned()
            }
            AppError::Internal(error) => {
                tracing::error!(%error, "request failed");
                "internal error".to_owned()
            }
            other => other.to_string(),
        };
        if matches!(self, AppError::PaymentRequired(_)) {
            return (
                status,
                Json(json!({ "error": message, "subscription": "required" })),
            )
                .into_response();
        }
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
