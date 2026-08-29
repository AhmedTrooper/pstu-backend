use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ErrorEnvelope {
    pub error: ErrorDetails,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ErrorDetails {
    pub code: String,
    pub message: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<HashMap<String, String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Validation failed")]
    Validation {
        message: String,
        fields: HashMap<String, String>,
    },

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Resource expired/gone: {0}")]
    Gone(String),

    #[error("Payload too large")]
    PayloadTooLarge(String),

    #[error("Unprocessable entity: {0}")]
    Unprocessable(String),

    #[error("Too many requests")]
    RateLimited(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Validation { .. } => StatusCode::BAD_REQUEST,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Gone(_) => StatusCode::GONE,
            AppError::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Redis(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::BadRequest(_) => "BAD_REQUEST",
            AppError::Validation { .. } => "VALIDATION_FAILED",
            AppError::Unauthorized(_) => "UNAUTHORIZED",
            AppError::Forbidden(_) => "FORBIDDEN",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::Conflict(_) => "CONFLICT",
            AppError::Gone(_) => "EXPIRED",
            AppError::PayloadTooLarge(_) => "PAYLOAD_TOO_LARGE",
            AppError::Unprocessable(_) => "UNPROCESSABLE_ENTITY",
            AppError::RateLimited(_) => "RATE_LIMITED",
            AppError::ServiceUnavailable(_) => "SERVICE_UNAVAILABLE",
            AppError::Internal(_) => "INTERNAL_SERVER_ERROR",
            AppError::Database(_) => "DATABASE_ERROR",
            AppError::Redis(_) => "CACHE_UNAVAILABLE",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code().to_string();
        let message = match &self {
            AppError::Database(err) => {
                error!(error = ?err, "Internal database error occurred");
                "An unexpected database error occurred. Please try again later.".to_string()
            }
            AppError::Internal(msg) => {
                error!(msg = %msg, "Internal server error occurred");
                "An internal error occurred. Please try again later.".to_string()
            }
            AppError::Validation { message, .. } => message.clone(),
            _ => self.to_string(),
        };

        let fields = match &self {
            AppError::Validation { fields, .. } => Some(fields.clone()),
            _ => None,
        };

        // Fallback request ID if not extracted from span
        let request_id = uuid::Uuid::new_v4().to_string();

        let body = Json(ErrorEnvelope {
            error: ErrorDetails {
                code,
                message,
                request_id,
                fields,
            },
        });

        (status, body).into_response()
    }
}
