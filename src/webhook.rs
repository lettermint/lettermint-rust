//! Webhook verification: [`Webhook`] checks the `X-Lettermint-Signature` of a delivery.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::BuildHasher;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::{ConfigError, Result};
use crate::generated::types::WebhookEvent;
use crate::tokens::{REDACTED, Secret};

/// The signature header, `t=<unix seconds>,v1=<hex HMAC-SHA256>`.
pub const SIGNATURE_HEADER: &str = "x-lettermint-signature";
/// The delivery header. It holds the signed timestamp.
pub const DELIVERY_HEADER: &str = "x-lettermint-delivery";
/// The default timestamp tolerance: 300 seconds in either direction.
pub const DEFAULT_TOLERANCE: Duration = Duration::from_secs(300);

/// A verified webhook delivery. Unknown top-level fields are kept in `extra`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WebhookPayload {
    /// The delivery ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The event, for example `message.delivered`. Events the SDK does not know are kept as
    /// [`WebhookEvent::Other`].
    pub event: WebhookEvent,
    /// When the event occurred (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// The event data. Decode it with [`WebhookPayload::data_as`].
    #[serde(default)]
    pub data: serde_json::Value,
    /// Other fields of the payload, such as `context`.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl WebhookPayload {
    /// Decodes [`data`](Self::data) into your own type.
    pub fn data_as<T: serde::de::DeserializeOwned>(&self) -> serde_json::Result<T> {
        T::deserialize(&self.data)
    }
}

/// Why a webhook delivery failed verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WebhookVerificationReason {
    /// `signature_header_missing`: no `X-Lettermint-Signature` header.
    SignatureHeaderMissing,
    /// `signature_header_malformed`: the signature header cannot be parsed, has non-ASCII
    /// characters, no `t`, more than one `t`, or no `v1`; or the request has the header twice.
    SignatureHeaderMalformed,
    /// `delivery_header_missing`: no `X-Lettermint-Delivery` header.
    DeliveryHeaderMissing,
    /// `delivery_timestamp_mismatch`: the delivery header differs from the signed timestamp.
    DeliveryTimestampMismatch,
    /// `timestamp_out_of_tolerance`: the signed timestamp is too far from the current time.
    TimestampOutOfTolerance,
    /// `signature_mismatch`: no `v1` signature matches the body.
    SignatureMismatch,
    /// `body_invalid`: the body is empty.
    BodyInvalid,
    /// `payload_invalid`: the body is not a JSON object with an `event`.
    PayloadInvalid,
}

impl WebhookVerificationReason {
    /// The reason code, shared by every Lettermint SDK.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SignatureHeaderMissing => "signature_header_missing",
            Self::SignatureHeaderMalformed => "signature_header_malformed",
            Self::DeliveryHeaderMissing => "delivery_header_missing",
            Self::DeliveryTimestampMismatch => "delivery_timestamp_mismatch",
            Self::TimestampOutOfTolerance => "timestamp_out_of_tolerance",
            Self::SignatureMismatch => "signature_mismatch",
            Self::BodyInvalid => "body_invalid",
            Self::PayloadInvalid => "payload_invalid",
        }
    }
}

impl fmt::Display for WebhookVerificationReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A webhook delivery could not be verified. Reject the request; do not process its payload.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct WebhookVerificationError {
    reason: WebhookVerificationReason,
    message: String,
}

impl WebhookVerificationError {
    fn new(reason: WebhookVerificationReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }

    /// The machine-readable reason.
    pub fn reason(&self) -> WebhookVerificationReason {
        self.reason
    }

    /// The message. It never contains the secret.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Request headers that [`Webhook::verify`] can read: `http::HeaderMap` (axum, hyper, reqwest),
/// `HashMap` and `BTreeMap` of strings, and slices, arrays or vectors of `(name, value)` pairs.
/// Names are compared case-insensitively.
pub trait WebhookHeaders {
    /// Every value of the header `name` (lowercase), in order. `None` stands for a value that is
    /// not valid text.
    fn header_values(&self, name: &str) -> Vec<Option<String>>;
}

impl<T: WebhookHeaders + ?Sized> WebhookHeaders for &T {
    fn header_values(&self, name: &str) -> Vec<Option<String>> {
        (**self).header_values(name)
    }
}

impl WebhookHeaders for http::HeaderMap {
    fn header_values(&self, name: &str) -> Vec<Option<String>> {
        self.get_all(name)
            .iter()
            .map(|value| value.to_str().ok().map(str::to_owned))
            .collect()
    }
}

fn pairs_values<'a, K, V>(
    pairs: impl IntoIterator<Item = (&'a K, &'a V)>,
    name: &str,
) -> Vec<Option<String>>
where
    K: AsRef<str> + ?Sized + 'a,
    V: AsRef<str> + ?Sized + 'a,
{
    pairs
        .into_iter()
        .filter(|(key, _)| key.as_ref().eq_ignore_ascii_case(name))
        .map(|(_, value)| Some(value.as_ref().to_owned()))
        .collect()
}

impl<K: AsRef<str>, V: AsRef<str>, S: BuildHasher> WebhookHeaders for HashMap<K, V, S> {
    fn header_values(&self, name: &str) -> Vec<Option<String>> {
        pairs_values(self.iter(), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> WebhookHeaders for BTreeMap<K, V> {
    fn header_values(&self, name: &str) -> Vec<Option<String>> {
        pairs_values(self.iter(), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> WebhookHeaders for [(K, V)] {
    fn header_values(&self, name: &str) -> Vec<Option<String>> {
        pairs_values(self.iter().map(|(key, value)| (key, value)), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>, const N: usize> WebhookHeaders for [(K, V); N] {
    fn header_values(&self, name: &str) -> Vec<Option<String>> {
        self.as_slice().header_values(name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> WebhookHeaders for Vec<(K, V)> {
    fn header_values(&self, name: &str) -> Vec<Option<String>> {
        self.as_slice().header_values(name)
    }
}

/// Verifies Lettermint webhook deliveries: an HMAC-SHA256 signature over `"<t>." + raw body`
/// with the endpoint's signing secret (`whsec_…`, used as given), compared in constant time.
///
/// ```no_run
/// use lettermint::{Webhook, WebhookVerificationError};
///
/// # fn handle(raw_body: &[u8], headers: &http::HeaderMap) -> Result<(), Box<dyn std::error::Error>> {
/// let webhook = Webhook::new(std::env::var("LETTERMINT_WEBHOOK_SECRET")?)?;
/// match webhook.verify(raw_body, headers) {
///     Ok(payload) => println!("{} {}", payload.event, payload.data),
///     Err(error) => eprintln!("rejected: {}", error.reason()), // answer 400
/// }
/// # Ok(())
/// # }
/// ```
///
/// Its `Debug` output shows the secret as `[redacted]`.
#[derive(Clone)]
pub struct Webhook {
    secret: Secret,
    tolerance: Duration,
}

impl Webhook {
    /// A verifier for one webhook endpoint. Returns an [`Error::Config`](crate::Error::Config)
    /// when the secret is empty.
    pub fn new(secret: impl Into<String>) -> Result<Self> {
        let secret = secret.into();
        if secret.is_empty() {
            return Err(ConfigError::new("The webhook signing secret must not be empty.").into());
        }
        Ok(Self {
            secret: Secret(secret),
            tolerance: DEFAULT_TOLERANCE,
        })
    }

    /// The maximum difference between the signed timestamp and the current time, in either
    /// direction, in whole seconds. Default 300 seconds. Zero accepts only the current second; it
    /// does not disable the check.
    pub fn with_tolerance(mut self, tolerance: Duration) -> Self {
        self.tolerance = Duration::from_secs(tolerance.as_secs());
        self
    }

    /// The timestamp tolerance.
    pub fn tolerance(&self) -> Duration {
        self.tolerance
    }

    /// Verifies a delivery from its raw body and request headers, and returns the payload.
    ///
    /// Requires `X-Lettermint-Signature` and `X-Lettermint-Delivery`, which must equal the
    /// signed timestamp. Pass the body exactly as received: the signature covers its bytes, so
    /// parsing and re-serializing the JSON breaks it.
    pub fn verify(
        &self,
        raw_body: impl AsRef<[u8]>,
        headers: &(impl WebhookHeaders + ?Sized),
    ) -> Result<WebhookPayload, WebhookVerificationError> {
        self.verify_at(raw_body, headers, SystemTime::now())
    }

    /// [`verify`](Self::verify) with `now` as the current time, for example to replay a stored
    /// delivery in a test.
    pub fn verify_at(
        &self,
        raw_body: impl AsRef<[u8]>,
        headers: &(impl WebhookHeaders + ?Sized),
        now: SystemTime,
    ) -> Result<WebhookPayload, WebhookVerificationError> {
        use WebhookVerificationReason::*;
        let signature = match headers.header_values(SIGNATURE_HEADER).as_slice() {
            [] => {
                return Err(WebhookVerificationError::new(
                    SignatureHeaderMissing,
                    "The X-Lettermint-Signature header is missing.",
                ));
            }
            [Some(value)] => value.clone(),
            [None] => return Err(malformed("it is not valid ASCII")),
            _ => {
                return Err(WebhookVerificationError::new(
                    SignatureHeaderMalformed,
                    "The request has more than one X-Lettermint-Signature header.",
                ));
            }
        };
        let delivery = match headers.header_values(DELIVERY_HEADER).as_slice() {
            [] => {
                return Err(WebhookVerificationError::new(
                    DeliveryHeaderMissing,
                    "The X-Lettermint-Delivery header is missing.",
                ));
            }
            [Some(value)] => value.clone(),
            [None] => {
                return Err(WebhookVerificationError::new(
                    DeliveryTimestampMismatch,
                    "The X-Lettermint-Delivery header does not match the signed timestamp.",
                ));
            }
            _ => {
                return Err(WebhookVerificationError::new(
                    DeliveryTimestampMismatch,
                    "The request has more than one X-Lettermint-Delivery header.",
                ));
            }
        };
        self.verify_signature_at(raw_body, &signature, Some(&delivery), now)
    }

    /// Verifies the raw body against an `X-Lettermint-Signature` value, for setups where the
    /// headers are not at hand. When `delivery` (the `X-Lettermint-Delivery` value) is given, it
    /// must equal the signed timestamp.
    pub fn verify_signature(
        &self,
        raw_body: impl AsRef<[u8]>,
        signature_header: &str,
        delivery: Option<&str>,
    ) -> Result<WebhookPayload, WebhookVerificationError> {
        self.verify_signature_at(raw_body, signature_header, delivery, SystemTime::now())
    }

    /// [`verify_signature`](Self::verify_signature) with `now` as the current time.
    pub fn verify_signature_at(
        &self,
        raw_body: impl AsRef<[u8]>,
        signature_header: &str,
        delivery: Option<&str>,
        now: SystemTime,
    ) -> Result<WebhookPayload, WebhookVerificationError> {
        use WebhookVerificationReason::*;
        if signature_header.trim().is_empty() {
            return Err(WebhookVerificationError::new(
                SignatureHeaderMissing,
                "The X-Lettermint-Signature header is missing.",
            ));
        }
        let (timestamp, signatures) = parse_signature_header(signature_header)?;
        if let Some(delivery) = delivery
            && delivery.trim() != timestamp
        {
            return Err(WebhookVerificationError::new(
                DeliveryTimestampMismatch,
                "The X-Lettermint-Delivery header does not match the signed timestamp.",
            ));
        }
        let body = raw_body.as_ref();
        if body.is_empty() {
            return Err(WebhookVerificationError::new(
                BodyInvalid,
                "The raw request body is empty.",
            ));
        }
        let signed_at: i64 = timestamp
            .parse()
            .map_err(|_| malformed("the timestamp is not a number of seconds"))?;
        let now = match now.duration_since(UNIX_EPOCH) {
            Ok(elapsed) => elapsed.as_secs() as i64,
            Err(before) => -(before.duration().as_secs_f64().ceil() as i64),
        };
        if now.abs_diff(signed_at) > self.tolerance.as_secs() {
            return Err(WebhookVerificationError::new(
                TimestampOutOfTolerance,
                "The signed timestamp is outside the allowed tolerance.",
            ));
        }

        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.expose().as_bytes())
            .expect("HMAC accepts keys of any length");
        mac.update(timestamp.as_bytes());
        mac.update(b".");
        mac.update(body);
        let expected = mac.finalize().into_bytes();
        let mut matched = false;
        for candidate in &signatures {
            matched |= constant_time_eq(candidate, &expected);
        }
        if !matched {
            return Err(WebhookVerificationError::new(
                SignatureMismatch,
                "The webhook signature does not match.",
            ));
        }

        let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| {
            WebhookVerificationError::new(PayloadInvalid, "The webhook payload is not valid JSON.")
        })?;
        if !value.is_object() {
            return Err(WebhookVerificationError::new(
                PayloadInvalid,
                "The webhook payload is not a JSON object.",
            ));
        }
        serde_json::from_value(value).map_err(|_| {
            WebhookVerificationError::new(
                PayloadInvalid,
                "The webhook payload has no valid `event`.",
            )
        })
    }
}

impl fmt::Debug for Webhook {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Webhook")
            .field("secret", &REDACTED)
            .field("tolerance", &self.tolerance)
            .finish()
    }
}

fn malformed(detail: &str) -> WebhookVerificationError {
    WebhookVerificationError::new(
        WebhookVerificationReason::SignatureHeaderMalformed,
        format!("The signature header is malformed: {detail}."),
    )
}

/// `(t, [v1 signatures])`. Unknown schemes and `v1` values that are not 64 hex characters are
/// ignored.
fn parse_signature_header(
    header: &str,
) -> Result<(String, Vec<Vec<u8>>), WebhookVerificationError> {
    if !header.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        return Err(malformed("it contains non-ASCII or control characters"));
    }
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for part in header.split(',') {
        let Some((key, value)) = part.trim().split_once('=') else {
            continue;
        };
        match key {
            "t" => {
                if timestamp.is_some() {
                    return Err(malformed("it has more than one timestamp"));
                }
                // At most 2^53 - 1, like every other Lettermint SDK.
                let valid = !value.is_empty()
                    && value.bytes().all(|byte| byte.is_ascii_digit())
                    && value
                        .parse::<u64>()
                        .is_ok_and(|seconds| seconds < (1 << 53));
                if !valid {
                    return Err(malformed("the timestamp is not a number of seconds"));
                }
                timestamp = Some(value.to_owned());
            }
            "v1" if value.len() == 64 => {
                if let Ok(bytes) = hex::decode(value) {
                    signatures.push(bytes);
                }
            }
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or_else(|| malformed("the timestamp (t=) is missing"))?;
    if signatures.is_empty() {
        return Err(malformed("no v1 signature is present"));
    }
    Ok((timestamp, signatures))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_unit_test_secret";

    fn sign(timestamp: i64, body: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(format!("{timestamp}.{body}").as_bytes());
        format!(
            "t={timestamp},v1={}",
            hex::encode(mac.finalize().into_bytes())
        )
    }

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn verifies_and_decodes_payloads() {
        let body = r#"{"id":"d1","event":"message.delivered","timestamp":"2026-10-03T12:00:00Z","data":{"message_id":"m1"},"context":{"project_id":"p1"}}"#;
        let webhook = Webhook::new(SECRET).unwrap();
        let headers = [
            ("X-Lettermint-Signature", sign(1_000, body)),
            ("X-Lettermint-Delivery", "1000".to_owned()),
        ];
        let payload = webhook.verify_at(body, &headers, at(1_000)).unwrap();
        assert_eq!(payload.id.as_deref(), Some("d1"));
        assert_eq!(payload.event, WebhookEvent::MessageDelivered);
        assert_eq!(payload.extra["context"]["project_id"], "p1");
        #[derive(Deserialize)]
        struct Delivered {
            message_id: String,
        }
        assert_eq!(payload.data_as::<Delivered>().unwrap().message_id, "m1");
    }

    #[test]
    fn unknown_events_are_kept() {
        let body = r#"{"event":"message.teleported","data":{}}"#;
        let payload = Webhook::new(SECRET)
            .unwrap()
            .verify_signature_at(body, &sign(5, body), None, at(5))
            .unwrap();
        assert_eq!(payload.event.as_str(), "message.teleported");
        assert!(!payload.event.is_known());
    }

    #[test]
    fn failures_have_reasons() {
        use WebhookVerificationReason::*;
        let body = r#"{"event":"webhook.test","data":{}}"#;
        let webhook = Webhook::new(SECRET).unwrap();
        let check = |signature: &str, delivery: Option<&str>, body: &str, now: u64| {
            webhook
                .verify_signature_at(body, signature, delivery, at(now))
                .unwrap_err()
                .reason()
        };
        assert_eq!(check("", None, body, 100), SignatureHeaderMissing);
        assert_eq!(check("t=100", None, body, 100), SignatureHeaderMalformed);
        assert_eq!(
            check("t=100,t=100,v1=00", None, body, 100),
            SignatureHeaderMalformed
        );
        assert_eq!(
            check(&sign(100, body), Some("101"), body, 100),
            DeliveryTimestampMismatch
        );
        assert_eq!(check(&sign(100, body), None, "", 100), BodyInvalid);
        assert_eq!(
            check(&sign(100, body), None, body, 401),
            TimestampOutOfTolerance
        );
        assert!(
            webhook
                .verify_signature_at(body, &sign(100, body), Some(" 100 "), at(400))
                .is_ok()
        );
        assert!(
            webhook
                .verify_signature_at(body, &sign(400, body), None, at(100))
                .is_ok()
        );
        assert_eq!(
            check(&sign(401, body), None, body, 100),
            TimestampOutOfTolerance
        );
        assert_eq!(check(&sign(100, "[]"), None, "[]", 100), PayloadInvalid);
        assert_eq!(check(&sign(100, "{}"), None, "{}", 100), PayloadInvalid);
        assert_eq!(check(&sign(100, "nope"), None, "nope", 100), PayloadInvalid);
        assert_eq!(
            check(&sign(100, body), None, &body.replace("test", "tset"), 100),
            SignatureMismatch
        );
    }

    #[test]
    fn header_lookup_is_case_insensitive_and_strict() {
        use WebhookVerificationReason::*;
        let body = r#"{"event":"webhook.test","data":{}}"#;
        let webhook = Webhook::new(SECRET).unwrap();
        let signature = sign(100, body);
        let mut map = http::HeaderMap::new();
        map.insert("X-LETTERMINT-SIGNATURE", signature.parse().unwrap());
        map.insert("x-lettermint-delivery", "100".parse().unwrap());
        assert!(webhook.verify_at(body, &map, at(100)).is_ok());
        map.append("x-lettermint-signature", signature.parse().unwrap());
        assert_eq!(
            webhook.verify_at(body, &map, at(100)).unwrap_err().reason(),
            SignatureHeaderMalformed
        );

        let mut map = http::HeaderMap::new();
        map.insert(
            "x-lettermint-signature",
            http::HeaderValue::from_bytes("t=1,v1=é".as_bytes()).unwrap(),
        );
        map.insert("x-lettermint-delivery", "1".parse().unwrap());
        assert_eq!(
            webhook.verify_at(body, &map, at(1)).unwrap_err().reason(),
            SignatureHeaderMalformed
        );

        let hash: HashMap<String, String> =
            [("x-lettermint-signature".to_owned(), signature.clone())].into();
        assert_eq!(
            webhook
                .verify_at(body, &hash, at(100))
                .unwrap_err()
                .reason(),
            DeliveryHeaderMissing
        );
        let pairs = vec![
            ("X-Lettermint-Delivery", "100"),
            ("x-lettermint-delivery", "100"),
            ("X-Lettermint-Signature", signature.as_str()),
        ];
        assert_eq!(
            webhook
                .verify_at(body, &pairs, at(100))
                .unwrap_err()
                .reason(),
            DeliveryTimestampMismatch
        );
    }

    #[test]
    fn configuration_is_checked_and_the_secret_never_shows() {
        assert!(Webhook::new("").is_err());
        let webhook = Webhook::new(SECRET)
            .unwrap()
            .with_tolerance(Duration::from_millis(60_900));
        assert_eq!(webhook.tolerance(), Duration::from_secs(60));
        for output in [format!("{webhook:?}"), format!("{webhook:#?}")] {
            assert!(!output.contains(SECRET), "{output}");
        }
        let error = webhook
            .verify_signature("{}", "t=1,v1=00", None)
            .unwrap_err();
        assert!(!format!("{error:?} {error}").contains(SECRET));
    }
}
