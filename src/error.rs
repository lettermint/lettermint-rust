use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("API token is required")]
    MissingToken,
    #[error("HTTP error {status}: {message}")]
    Http {
        status: u16,
        message: String,
        body: Option<serde_json::Value>,
    },
    #[error("validation error: {error_type}")]
    Validation {
        error_type: String,
        body: Option<serde_json::Value>,
    },
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("invalid HTTP header: {0}")]
    InvalidHeader(String),
    #[error("failed to decode JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid webhook signature")]
    InvalidSignature,
    #[error("invalid webhook signature format")]
    InvalidSignatureFormat,
    #[error("timestamp mismatch between signature and delivery header")]
    TimestampMismatch,
    #[error(
        "timestamp outside tolerance: difference {difference_seconds}s, tolerance {tolerance_seconds}s"
    )]
    TimestampOutsideTolerance {
        difference_seconds: i64,
        tolerance_seconds: i64,
    },
    #[error("webhook secret is required")]
    MissingWebhookSecret,
    #[error("invalid webhook payload JSON: {0}")]
    InvalidWebhookJson(#[source] serde_json::Error),
    #[error("invalid HMAC key")]
    InvalidHmacKey,
}
