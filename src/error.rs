//! The error type of the SDK.
//!
//! Every fallible call returns [`Error`]. No error carries request headers or API tokens:
//! the SDK removes the configured tokens from error bodies and messages.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use crate::webhook::WebhookVerificationError;

/// `Result<T, lettermint::Error>`.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A boxed error from a transport, kept as the source of [`Error::Connection`].
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Every error the SDK returns.
///
/// Match the variants you handle and keep a wildcard arm: new variants may be added in minor
/// releases. [`Error::status`] and [`Error::code`] work across variants.
///
/// The HTTP variants (`Authentication` through `Api`) all carry an [`ApiError`]; use
/// [`Error::api_error`] to read it whatever the status.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The client was configured or called incorrectly: a missing or unrecognised token, a token
    /// that the called method cannot use, an invalid option or an invalid ID. Returned before
    /// any request is made.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// The SDK rejected a request before sending it, for example invalid message tags. Unlike
    /// [`Error::Validation`], the API never saw this request.
    #[error(transparent)]
    InvalidInput(#[from] InputError),

    /// HTTP 401: the token is missing, invalid or revoked.
    #[error("{0}")]
    Authentication(ApiError),

    /// HTTP 403: the token may not perform this action, or the plan lacks the feature.
    #[error("{0}")]
    Permission(ApiError),

    /// HTTP 404: the resource does not exist or is not visible to the token.
    #[error("{0}")]
    NotFound(ApiError),

    /// HTTP 409: the request conflicts with the current state, for example an
    /// `Idempotency-Key` reused with a different body.
    #[error("{0}")]
    Conflict(ApiError),

    /// HTTP 422: the API rejected the request data. [`ApiError::errors`] holds the field errors.
    #[error("{0}")]
    Validation(ApiError),

    /// HTTP 429: too many requests. [`ApiError::retry_after`] holds the `Retry-After` delay.
    #[error("{0}")]
    RateLimit(ApiError),

    /// HTTP 5xx with a JSON or empty body.
    #[error("{0}")]
    Server(ApiError),

    /// Any other 4xx status with a JSON or empty body.
    #[error("{0}")]
    Api(ApiError),

    /// No complete response arrived within the timeout. The timeout covers the response
    /// headers and the body. The API may still have processed the request.
    #[error("The request to the Lettermint API timed out after {} ms.", .timeout.as_millis())]
    #[non_exhaustive]
    Timeout {
        /// The timeout that expired.
        timeout: Duration,
    },

    /// The request could not be sent or the connection failed (DNS, TLS, refused, reset).
    #[error("Could not reach the Lettermint API: {source}")]
    #[non_exhaustive]
    Connection {
        /// The transport's error.
        #[source]
        source: BoxError,
    },

    /// The response could not be decoded: an empty or non-JSON body where JSON was expected, a
    /// body that does not match the documented type, or an error status with a non-JSON body
    /// such as a proxy's HTML page.
    #[error(transparent)]
    UnexpectedResponse(UnexpectedResponseError),

    /// The API answered with a redirect (3xx). The SDK never follows redirects, so that tokens
    /// are not sent to another location.
    #[error(
        "The Lettermint API answered with a redirect (HTTP {status}). Redirects are not followed; check the base_url option."
    )]
    #[non_exhaustive]
    Redirect {
        /// The HTTP status code.
        status: u16,
    },

    /// A webhook delivery could not be verified. Reject the request; do not process its payload.
    #[error(transparent)]
    WebhookVerification(#[from] WebhookVerificationError),
}

impl Error {
    /// A connection error with `source` as its cause. For [`Transport`](crate::Transport)
    /// implementations.
    pub fn connection(source: impl Into<BoxError>) -> Self {
        Self::Connection {
            source: source.into(),
        }
    }

    /// A timeout error. For [`Transport`](crate::Transport) implementations that enforce
    /// [`HttpRequest::timeout`](crate::HttpRequest::timeout) themselves.
    pub fn timeout(timeout: Duration) -> Self {
        Self::Timeout { timeout }
    }

    /// The HTTP status code, for HTTP errors, unexpected responses and redirects.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::UnexpectedResponse(error) => Some(error.status),
            Self::Redirect { status } => Some(*status),
            _ => self.api_error().map(ApiError::status),
        }
    }

    /// The API's machine-readable error code (`{ "error": { "code": ... } }` or a string
    /// `error` field), for HTTP errors.
    pub fn code(&self) -> Option<&str> {
        self.api_error().and_then(ApiError::code)
    }

    /// The [`ApiError`] of any HTTP error variant.
    pub fn api_error(&self) -> Option<&ApiError> {
        match self {
            Self::Authentication(error)
            | Self::Permission(error)
            | Self::NotFound(error)
            | Self::Conflict(error)
            | Self::Validation(error)
            | Self::RateLimit(error)
            | Self::Server(error)
            | Self::Api(error) => Some(error),
            _ => None,
        }
    }

    /// Whether the outcome of the request is unknown and a retry with the same idempotency key is
    /// reasonable: a timeout, a connection failure, HTTP 429 or a 5xx response.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout { .. } | Self::Connection { .. } | Self::RateLimit(_) | Self::Server(_)
        ) || matches!(self, Self::UnexpectedResponse(error) if error.status >= 500)
    }
}

/// A configuration error. See [`Error::Config`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The message. It never contains a token.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// A request the SDK rejected before sending it. See [`Error::InvalidInput`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct InputError {
    field: String,
    message: String,
}

impl InputError {
    pub(crate) fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }

    /// The offending field, for example `tags` or `messages[2].tags`.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// An error response from the API (4xx or 5xx with a JSON or empty body).
#[derive(Clone, PartialEq)]
pub struct ApiError {
    inner: Box<ApiErrorFields>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ApiErrorFields {
    pub(crate) status: u16,
    pub(crate) code: Option<String>,
    pub(crate) message: String,
    pub(crate) details: Option<serde_json::Value>,
    pub(crate) body: Option<serde_json::Value>,
    pub(crate) errors: Option<BTreeMap<String, Vec<String>>>,
    pub(crate) retry_after: Option<Duration>,
}

impl ApiError {
    pub(crate) fn new(fields: ApiErrorFields) -> Self {
        Self {
            inner: Box::new(fields),
        }
    }

    /// The HTTP status code.
    pub fn status(&self) -> u16 {
        self.inner.status
    }

    /// The machine-readable error code from `{ "error": { "code" } }`, or a string `error`
    /// field, if the API sent one.
    pub fn code(&self) -> Option<&str> {
        self.inner.code.as_deref()
    }

    /// The API's message, or the HTTP status text.
    pub fn message(&self) -> &str {
        &self.inner.message
    }

    /// Additional context from `{ "error": { "details" } }`, if the API sent any.
    pub fn details(&self) -> Option<&serde_json::Value> {
        self.inner.details.as_ref()
    }

    /// The decoded JSON error body, or `None` for an empty body.
    pub fn body(&self) -> Option<&serde_json::Value> {
        self.inner.body.as_ref()
    }

    /// Field errors from a Laravel-style `{ "message", "errors" }` body (HTTP 422).
    pub fn errors(&self) -> Option<&BTreeMap<String, Vec<String>>> {
        self.inner.errors.as_ref()
    }

    /// How long to wait, from the `Retry-After` header (seconds or an HTTP date), when the API
    /// sent one (HTTP 429).
    pub fn retry_after(&self) -> Option<Duration> {
        self.inner.retry_after
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} (HTTP {}",
            self.inner.message, self.inner.status
        )?;
        if let Some(code) = &self.inner.code {
            write!(formatter, ", {code}")?;
        }
        formatter.write_str(")")
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiError")
            .field("status", &self.inner.status)
            .field("code", &self.inner.code)
            .field("message", &self.inner.message)
            .field("details", &self.inner.details)
            .field("body", &self.inner.body)
            .field("errors", &self.inner.errors)
            .field("retry_after", &self.inner.retry_after)
            .finish()
    }
}

impl std::error::Error for ApiError {}

/// A response the SDK could not decode. See [`Error::UnexpectedResponse`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct UnexpectedResponseError {
    pub(crate) status: u16,
    pub(crate) message: String,
    pub(crate) body_excerpt: String,
}

impl UnexpectedResponseError {
    /// The HTTP status code.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The first 200 characters of the response body.
    pub fn body_excerpt(&self) -> &str {
        &self.body_excerpt
    }
}
