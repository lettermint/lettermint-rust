//! The HTTP layer: the [`Transport`] trait, the default [`ReqwestTransport`], and request
//! execution (auth headers, path and query encoding, timeouts and response decoding).

use std::fmt;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use http::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, USER_AGENT,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::client::Lettermint;
use crate::error::{
    ApiError, ApiErrorFields, ConfigError, Error, InputError, Result, UnexpectedResponseError,
};
use crate::generated::operations::{AuthSurface, OperationDefinition, ResponseKind};
use crate::generated::support::QueryValue;
use crate::query;
use crate::tokens::REDACTED;

/// The default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.lettermint.co/v1";

/// The default request timeout. It covers the whole request, including reading the body.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The `User-Agent` header the SDK sends.
pub const USER_AGENT_VALUE: &str = concat!("lettermint-rust/", env!("CARGO_PKG_VERSION"));

const SENDING_TOKEN_HEADER: &str = "x-lettermint-token";
const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";

/// One HTTP request, as the SDK hands it to a [`Transport`].
///
/// The `Debug` output shows the `Authorization` and `x-lettermint-token` headers as
/// `[redacted]`.
#[derive(Clone)]
#[non_exhaustive]
pub struct HttpRequest {
    /// The HTTP method.
    pub method: http::Method,
    /// The absolute URL, including the query string.
    pub url: String,
    /// The request headers, including the token. The token header is marked sensitive.
    pub headers: HeaderMap,
    /// The JSON body, if the operation has one.
    pub body: Option<Vec<u8>>,
    /// The time the whole request may take, including reading the response body. The SDK also
    /// enforces it around [`Transport::send`].
    pub timeout: Duration,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(name, value)| {
                let shown = if is_secret_header(name) || value.is_sensitive() {
                    REDACTED
                } else {
                    value.to_str().unwrap_or("<non-ASCII>")
                };
                (name.as_str(), shown)
            })
            .collect();
        formatter
            .debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field(
                "body",
                &self.body.as_ref().map(|body| String::from_utf8_lossy(body)),
            )
            .field("timeout", &self.timeout)
            .finish()
    }
}

fn is_secret_header(name: &HeaderName) -> bool {
    name == AUTHORIZATION || name.as_str() == SENDING_TOKEN_HEADER
}

/// One HTTP response, with the whole body read.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct HttpResponse {
    /// The HTTP status code.
    pub status: u16,
    /// The response headers.
    pub headers: HeaderMap,
    /// The response body.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// A response with `status`, `headers` and `body`.
    pub fn new(status: u16, headers: HeaderMap, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers,
            body: body.into(),
        }
    }
}

/// Sends HTTP requests for the SDK. Implement it to use another HTTP client or to test without
/// a network.
///
/// An implementation must:
///
/// - send the request as given and read the whole response body;
/// - **never follow redirects**: return the 3xx response, so that tokens never reach another
///   host (the SDK turns it into [`Error::Redirect`]);
/// - not retry;
/// - return [`Error::connection`] when the request fails, and [`Error::timeout`] when it enforces
///   [`HttpRequest::timeout`] itself.
///
/// The SDK enforces the timeout around `send` as well.
// `async_trait` marks the boxed future `#[must_use]`, which newer Clippy reports as `double_must_use`.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Sends `request` and returns the response with its whole body.
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse>;
}

/// The default transport, built on [`reqwest`] with rustls. It never follows redirects.
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// A transport with reqwest's defaults, the SDK's `User-Agent` and redirects disabled.
    pub fn new() -> Result<Self> {
        Self::from_builder(reqwest::Client::builder())
    }

    /// A transport from your own client builder, for example with a proxy or custom TLS roots.
    /// The SDK disables redirects on it, whatever the builder says.
    pub fn from_builder(builder: reqwest::ClientBuilder) -> Result<Self> {
        let client = builder
            .user_agent(USER_AGENT_VALUE)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| {
                ConfigError::new(format!("Could not create the HTTP client: {error}"))
            })?;
        Ok(Self { client })
    }
}

impl fmt::Debug for ReqwestTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReqwestTransport")
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
        let timeout = request.timeout;
        let mut builder = self
            .client
            .request(request.method, request.url)
            .headers(request.headers)
            .timeout(timeout);
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let map = |error: reqwest::Error| {
            if error.is_timeout() {
                Error::timeout(timeout)
            } else {
                Error::connection(error.without_url())
            }
        };
        let response = builder.send().await.map_err(map)?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.bytes().await.map_err(map)?;
        Ok(HttpResponse::new(status, headers, body.to_vec()))
    }
}

/// Per-call options inside the SDK.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallOptions {
    /// Overrides the auth surface from the operation table.
    pub auth: Option<AuthSurface>,
    pub idempotency_key: Option<String>,
    pub timeout: Option<Duration>,
}

impl Lettermint {
    /// The auth header for `auth`, or a config error that names the missing option.
    fn auth_header(&self, label: &str, auth: AuthSurface) -> Result<(HeaderName, HeaderValue)> {
        let core = self.core();
        let use_team = match auth {
            AuthSurface::Team => true,
            AuthSurface::Either => core.team_token.is_some(),
            AuthSurface::Sending => false,
        };
        let (name, value) = if use_team {
            let token = core.team_token.as_ref().ok_or_else(|| {
                ConfigError::new(format!(
                    "{label} needs `team_token`; set it with Lettermint::builder().team_token(..)."
                ))
            })?;
            (AUTHORIZATION, format!("Bearer {}", token.expose()))
        } else {
            let token = core.sending_token.as_ref().ok_or_else(|| {
                ConfigError::new(format!(
                    "{label} needs `sending_token`; set it with Lettermint::builder().sending_token(..)."
                ))
            })?;
            (
                HeaderName::from_static(SENDING_TOKEN_HEADER),
                token.expose().to_owned(),
            )
        };
        let mut value = HeaderValue::from_str(&value)
            .map_err(|_| ConfigError::new("The token is not a valid HTTP header value."))?;
        value.set_sensitive(true);
        Ok((name, value))
    }

    /// Returns a config error unless the token for `auth` is configured.
    pub(crate) fn assert_auth(&self, label: &str, auth: AuthSurface) -> Result<()> {
        self.auth_header(label, auth).map(|_| ())
    }

    /// Sends one operation and decodes the response as `T`.
    pub(crate) async fn execute<T, B>(
        &self,
        operation: &OperationDefinition,
        label: &str,
        path: &[&str],
        params: &[(String, QueryValue)],
        body: Option<&B>,
        options: CallOptions,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let (auth_name, auth_value) =
            self.auth_header(label, options.auth.unwrap_or(operation.auth))?;
        let path = build_path(operation, label, path)?;
        let timeout = options.timeout.unwrap_or(self.timeout());
        if timeout.is_zero() {
            return Err(ConfigError::new(format!(
                "{label}: the timeout must be greater than zero."
            ))
            .into());
        }

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        if let Some(key) = &options.idempotency_key {
            headers.insert(
                HeaderName::from_static(IDEMPOTENCY_KEY_HEADER),
                idempotency_header(key)?,
            );
        }
        let body = match body {
            Some(body) => {
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                Some(serde_json::to_vec(body).map_err(|error| {
                    InputError::new(
                        "body",
                        format!("The request body could not be encoded as JSON: {error}"),
                    )
                })?)
            }
            None => None,
        };
        headers.insert(auth_name, auth_value);

        let query = query::encode(params);
        let base_url = self.base_url();
        let url = if query.is_empty() {
            format!("{base_url}{path}")
        } else {
            format!("{base_url}{path}?{query}")
        };
        let method = http::Method::from_bytes(operation.method.as_bytes())
            .expect("the operation table holds valid methods");
        let request = HttpRequest {
            method,
            url,
            headers,
            body,
            timeout,
        };

        let response =
            match tokio::time::timeout(timeout, self.core().transport.send(request)).await {
                Ok(response) => response?,
                Err(_) => return Err(Error::timeout(timeout)),
            };
        self.decode(operation.response.kind, response)
    }

    fn decode<T: DeserializeOwned>(&self, kind: ResponseKind, response: HttpResponse) -> Result<T> {
        let status = response.status;
        let text = String::from_utf8_lossy(&response.body).into_owned();
        if (200..300).contains(&status) {
            let value = match kind {
                ResponseKind::Empty => serde_json::Value::Null,
                ResponseKind::Text => serde_json::Value::String(text.clone()),
                ResponseKind::Json(_) if text.trim().is_empty() => {
                    if status == 204 || status == 205 {
                        serde_json::Value::Null
                    } else {
                        return Err(self.unexpected(
                            status,
                            format!(
                                "The Lettermint API answered with HTTP {status} and an empty body where JSON was expected."
                            ),
                            &text,
                        ));
                    }
                }
                ResponseKind::Json(_) => serde_json::from_str(&text).map_err(|_| {
                    self.unexpected(
                        status,
                        format!("The Lettermint API answered with HTTP {status} and a body that is not valid JSON."),
                        &text,
                    )
                })?,
            };
            return serde_json::from_value(value).map_err(|error| {
                let expected = match kind {
                    ResponseKind::Json(name) => name,
                    ResponseKind::Text => "text",
                    ResponseKind::Empty => "no body",
                };
                self.unexpected(
                    status,
                    format!(
                        "The Lettermint API answered with HTTP {status} and a body that does not match {expected}: {error}"
                    ),
                    &text,
                )
            });
        }
        if (300..400).contains(&status) {
            return Err(Error::Redirect { status });
        }
        if status < 400 {
            return Err(self.unexpected(
                status,
                format!("The Lettermint API answered with an unexpected HTTP status {status}."),
                &text,
            ));
        }
        let body = if text.trim().is_empty() {
            None
        } else {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(value) => Some(self.redact_value(value)),
                Err(_) => {
                    let content_type = response
                        .headers
                        .get(CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.split(';').next())
                        .map(|value| format!(" ({})", value.trim()))
                        .unwrap_or_default();
                    return Err(self.unexpected(
                        status,
                        format!("The Lettermint API answered with HTTP {status} and a body that is not JSON{content_type}."),
                        &text,
                    ));
                }
            }
        };
        Err(api_error(
            status,
            &response.headers,
            body,
            SystemTime::now(),
        ))
    }

    fn unexpected(&self, status: u16, message: String, body: &str) -> Error {
        let body = self.redact(body);
        let body_excerpt = match body.char_indices().nth(200) {
            Some((index, _)) => format!("{}…", &body[..index]),
            None => body,
        };
        Error::UnexpectedResponse(UnexpectedResponseError {
            status,
            message: self.redact(&message),
            body_excerpt,
        })
    }

    /// Removes the configured tokens from text that came from the API.
    fn redact(&self, text: &str) -> String {
        let core = self.core();
        let mut text = text.to_owned();
        for token in [&core.sending_token, &core.team_token]
            .into_iter()
            .flatten()
        {
            if text.contains(token.expose()) {
                text = text.replace(token.expose(), REDACTED);
            }
        }
        text
    }

    fn redact_value(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(text) => serde_json::Value::String(self.redact(&text)),
            serde_json::Value::Array(items) => serde_json::Value::Array(
                items
                    .into_iter()
                    .map(|item| self.redact_value(item))
                    .collect(),
            ),
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .map(|(key, item)| (self.redact(&key), self.redact_value(item)))
                    .collect(),
            ),
            other => other,
        }
    }
}

fn idempotency_header(key: &str) -> Result<HeaderValue> {
    let invalid = || {
        InputError::new(
            "idempotency_key",
            "The idempotency key must be a non-empty string without line breaks.",
        )
    };
    if key.is_empty() || key.contains(['\r', '\n', '\0']) {
        return Err(invalid().into());
    }
    HeaderValue::from_str(key).map_err(|_| invalid().into())
}

/// Substitutes the encoded path parameters into the operation's path.
fn build_path(operation: &OperationDefinition, label: &str, values: &[&str]) -> Result<String> {
    if values.len() != operation.path_params.len() {
        return Err(ConfigError::new(format!(
            "{label}: expected {} path parameter(s).",
            operation.path_params.len()
        ))
        .into());
    }
    let mut path = operation.path.to_owned();
    for (name, value) in operation.path_params.iter().zip(values) {
        if value.is_empty() || *value == "." || *value == ".." {
            return Err(ConfigError::new(format!(
                "{label}: `{}` must be a non-empty string other than \".\" and \"..\".",
                snake_case(name)
            ))
            .into());
        }
        path = path.replace(&format!("{{{name}}}"), &query::encode_path_segment(value));
    }
    Ok(path)
}

fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for character in name.chars() {
        if character.is_ascii_uppercase() {
            out.push('_');
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// Builds the error for an error status with a JSON (or empty) body.
fn api_error(
    status: u16,
    headers: &HeaderMap,
    body: Option<serde_json::Value>,
    now: SystemTime,
) -> Error {
    let mut code = None;
    let mut message = None;
    let mut details = None;
    let mut errors = None;
    if let Some(serde_json::Value::Object(map)) = &body {
        match map.get("error") {
            Some(serde_json::Value::Object(error)) => {
                code = error
                    .get("code")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned);
                message = error
                    .get("message")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned);
                details = error.get("details").cloned();
            }
            Some(serde_json::Value::String(error)) => code = Some(error.clone()),
            _ => {}
        }
        if message.as_deref().is_none_or(str::is_empty) {
            message = map
                .get("message")
                .and_then(|value| value.as_str())
                .map(str::to_owned);
        }
        if let Some(serde_json::Value::Object(fields)) = map.get("errors") {
            errors = Some(
                fields
                    .iter()
                    .map(|(field, messages)| {
                        let messages = match messages {
                            serde_json::Value::Array(items) => items
                                .iter()
                                .filter_map(|item| item.as_str().map(str::to_owned))
                                .collect(),
                            serde_json::Value::String(text) => vec![text.clone()],
                            _ => Vec::new(),
                        };
                        (field.clone(), messages)
                    })
                    .collect(),
            );
        }
    }
    let message = message
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| {
            http::StatusCode::from_u16(status)
                .ok()
                .and_then(|status| status.canonical_reason())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("HTTP {status}"))
        });
    let retry_after = if status == 429 {
        headers
            .get(http::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| parse_retry_after(value, now))
    } else {
        None
    };
    let error = ApiError::new(ApiErrorFields {
        status,
        code,
        message,
        details,
        body,
        errors: if status == 422 { errors } else { None },
        retry_after,
    });
    match status {
        401 => Error::Authentication(error),
        403 => Error::Permission(error),
        404 => Error::NotFound(error),
        409 => Error::Conflict(error),
        422 => Error::Validation(error),
        429 => Error::RateLimit(error),
        500.. => Error::Server(error),
        _ => Error::Api(error),
    }
}

/// `Retry-After` as delay seconds or an HTTP date (IMF-fixdate).
fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        return value.parse().ok().map(Duration::from_secs);
    }
    let at = parse_http_date(value)?;
    let now = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs_f64();
    let seconds = (at as f64 - now).ceil().max(0.0);
    Some(Duration::from_secs(seconds as u64))
}

/// Parses an IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`) to Unix seconds.
fn parse_http_date(value: &str) -> Option<i64> {
    let (_, rest) = value.split_once(", ")?;
    let parts: Vec<&str> = rest.split(' ').collect();
    let [day, month, year, time, "GMT"] = parts.as_slice() else {
        return None;
    };
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|name| name == month)? as i64
        + 1;
    let day: i64 = day.parse().ok()?;
    let year: i64 = year.parse().ok()?;
    let mut clock = time.split(':').map(|part| part.parse::<i64>().ok());
    let (hour, minute, second) = (clock.next()??, clock.next()??, clock.next()??);
    if clock.next().is_some() || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60
    {
        return None;
    }
    // Days from civil (Howard Hinnant's algorithm).
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let year_of_era = shifted - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn parses_http_dates() {
        assert_eq!(
            parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(784_111_777)
        );
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(
            parse_http_date("Sat, 03 Oct 2026 12:00:00 GMT"),
            Some(1_791_028_800)
        );
        assert_eq!(parse_http_date("not a date"), None);
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 08:49:37 PST"), None);
    }

    #[test]
    fn retry_after_seconds_and_dates() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_028_790);
        assert_eq!(parse_retry_after("7", now), Some(Duration::from_secs(7)));
        assert_eq!(parse_retry_after(" 0 ", now), Some(Duration::ZERO));
        assert_eq!(
            parse_retry_after("Sat, 03 Oct 2026 12:00:00 GMT", now),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            parse_retry_after("Sat, 03 Oct 2026 11:00:00 GMT", now),
            Some(Duration::ZERO)
        );
        assert_eq!(parse_retry_after("soon", now), None);
        assert_eq!(parse_retry_after("-5", now), None);
    }

    #[test]
    fn api_errors_read_structured_and_laravel_bodies() {
        let now = SystemTime::now();
        let error = api_error(
            404,
            &HeaderMap::new(),
            Some(
                serde_json::json!({"error": {"code": "RESOURCE_NOT_FOUND", "message": "No such domain.", "details": {"id": "d1"}}}),
            ),
            now,
        );
        let Error::NotFound(api) = &error else {
            panic!("{error:?}")
        };
        assert_eq!(api.code(), Some("RESOURCE_NOT_FOUND"));
        assert_eq!(api.message(), "No such domain.");
        assert_eq!(api.details(), Some(&serde_json::json!({"id": "d1"})));
        assert_eq!(error.status(), Some(404));
        assert_eq!(error.code(), Some("RESOURCE_NOT_FOUND"));
        assert_eq!(
            error.to_string(),
            "No such domain. (HTTP 404, RESOURCE_NOT_FOUND)"
        );

        let error = api_error(
            422,
            &HeaderMap::new(),
            Some(
                serde_json::json!({"message": "The to field is required.", "errors": {"to": ["The to field is required."]}, "error": "ValidationError"}),
            ),
            now,
        );
        let Error::Validation(api) = &error else {
            panic!("{error:?}")
        };
        assert_eq!(api.code(), Some("ValidationError"));
        assert_eq!(api.message(), "The to field is required.");
        assert_eq!(
            api.errors().unwrap()["to"],
            vec!["The to field is required.".to_owned()]
        );

        let error = api_error(429, &headers(&[("retry-after", "12")]), None, now);
        let Error::RateLimit(api) = &error else {
            panic!("{error:?}")
        };
        assert_eq!(api.retry_after(), Some(Duration::from_secs(12)));
        assert_eq!(api.message(), "Too Many Requests");
        assert!(error.is_retryable());
    }

    #[test]
    fn statuses_map_to_variants() {
        let now = SystemTime::now();
        type Check = fn(&Error) -> bool;
        let kinds: Vec<(u16, Check)> = vec![
            (400, |e| matches!(e, Error::Api(_))),
            (401, |e| matches!(e, Error::Authentication(_))),
            (403, |e| matches!(e, Error::Permission(_))),
            (404, |e| matches!(e, Error::NotFound(_))),
            (409, |e| matches!(e, Error::Conflict(_))),
            (410, |e| matches!(e, Error::Api(_))),
            (422, |e| matches!(e, Error::Validation(_))),
            (429, |e| matches!(e, Error::RateLimit(_))),
            (500, |e| matches!(e, Error::Server(_))),
            (503, |e| matches!(e, Error::Server(_))),
        ];
        for (status, check) in kinds {
            let error = api_error(status, &HeaderMap::new(), None, now);
            assert!(check(&error), "{status}: {error:?}");
            assert_eq!(error.status(), Some(status));
        }
    }

    #[test]
    fn request_debug_output_redacts_tokens() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer lm_team_secret"),
        );
        headers.insert("x-lettermint-token", HeaderValue::from_static("lm_secret"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let request = HttpRequest {
            method: http::Method::POST,
            url: "https://api.lettermint.co/v1/send".into(),
            headers,
            body: Some(b"{}".to_vec()),
            timeout: DEFAULT_TIMEOUT,
        };
        for output in [format!("{request:?}"), format!("{request:#?}")] {
            assert!(!output.contains("lm_team_secret"), "{output}");
            assert!(!output.contains("lm_secret"), "{output}");
            assert!(output.contains("application/json"));
            assert!(output.contains("[redacted]"));
        }
    }

    #[test]
    fn idempotency_keys_must_be_header_safe() {
        assert!(idempotency_header("order-1").is_ok());
        for key in ["", "a\nb", "a\rb", "a\0b"] {
            assert!(
                matches!(idempotency_header(key), Err(Error::InvalidInput(_))),
                "{key:?}"
            );
        }
    }
}
