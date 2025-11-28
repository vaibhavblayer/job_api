// Error handling types for the API

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use std::fmt;
use tracing::error;

use super::validation::ValidationResult;

/// API error types
#[derive(Debug)]
pub enum ApiError {
    Unauthorized(String),
    Forbidden(String),
    BadRequest(String),
    NotFound(String),
    InternalServer(String),
    ServiceUnavailable(String),
    DatabaseError(sqlx::Error),
    ValidationError(String),
    BulkOperationError(String),
    ExportError(String),
    ProcessingError(String),
    AttachmentError(String),
    AnalyticsError(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            ApiError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            ApiError::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            ApiError::NotFound(msg) => write!(f, "Not Found: {}", msg),
            ApiError::InternalServer(msg) => write!(f, "Internal Server Error: {}", msg),
            ApiError::ServiceUnavailable(msg) => write!(f, "Service Unavailable: {}", msg),
            ApiError::DatabaseError(e) => write!(f, "Database Error: {}", e),
            ApiError::ValidationError(msg) => write!(f, "Validation Error: {}", msg),
            ApiError::BulkOperationError(msg) => write!(f, "Bulk Operation Error: {}", msg),
            ApiError::ExportError(msg) => write!(f, "Export Error: {}", msg),
            ApiError::ProcessingError(msg) => write!(f, "Processing Error: {}", msg),
            ApiError::AttachmentError(msg) => write!(f, "Attachment Error: {}", msg),
            ApiError::AnalyticsError(msg) => write!(f, "Analytics Error: {}", msg),
        }
    }
}

/// JSON error response structure
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message, code) = match self {
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg, "UNAUTHORIZED"),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg, "FORBIDDEN"),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg, "BAD_REQUEST"),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg, "NOT_FOUND"),
            ApiError::InternalServer(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                msg,
                "INTERNAL_SERVER_ERROR",
            ),
            ApiError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                msg,
                "SERVICE_UNAVAILABLE",
            ),
            ApiError::DatabaseError(e) => {
                error!(error = %e, "Database error occurred");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database operation failed".to_string(),
                    "DATABASE_ERROR",
                )
            }
            ApiError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg, "VALIDATION_ERROR"),
            ApiError::BulkOperationError(msg) => {
                (StatusCode::BAD_REQUEST, msg, "BULK_OPERATION_ERROR")
            }
            ApiError::ExportError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg, "EXPORT_ERROR"),
            ApiError::ProcessingError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg, "PROCESSING_ERROR")
            }
            ApiError::AttachmentError(msg) => (StatusCode::BAD_REQUEST, msg, "ATTACHMENT_ERROR"),
            ApiError::AnalyticsError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg, "ANALYTICS_ERROR")
            }
        };

        let error_response = ErrorResponse {
            error: error_message,
            code: code.to_string(),
        };

        (status, Json(error_response)).into_response()
    }
}

/// Helper function to convert ValidationResult to ApiError
impl From<ValidationResult> for ApiError {
    fn from(result: ValidationResult) -> Self {
        if result.is_valid {
            ApiError::InternalServer(
                "Validation result was valid but converted to error".to_string(),
            )
        } else {
            let error_messages: Vec<String> = result
                .errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect();
            ApiError::ValidationError(error_messages.join(", "))
        }
    }
}
