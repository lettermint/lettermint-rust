use crate::error::{Error, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub const SIGNATURE_HEADER: &str = "x-lettermint-signature";
pub const DELIVERY_HEADER: &str = "x-lettermint-delivery";
pub const DEFAULT_TOLERANCE_SECONDS: i64 = 300;

#[derive(Clone, Debug)]
pub struct Webhook {
    secret: String,
    tolerance_seconds: i64,
}

impl Webhook {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            tolerance_seconds: DEFAULT_TOLERANCE_SECONDS,
        }
    }

    pub fn with_tolerance(mut self, tolerance_seconds: i64) -> Self {
        self.tolerance_seconds = tolerance_seconds;
        self
    }

    pub fn verify(
        &self,
        payload: &str,
        signature: &str,
        delivery_timestamp: Option<i64>,
    ) -> Result<serde_json::Value> {
        if self.secret.is_empty() {
            return Err(Error::MissingWebhookSecret);
        }

        let (timestamp, signature) = parse_signature(signature)?;
        if delivery_timestamp.is_some_and(|delivery_timestamp| delivery_timestamp != timestamp) {
            return Err(Error::TimestampMismatch);
        }

        validate_timestamp(timestamp, self.tolerance_seconds)?;
        verify_signature(payload, timestamp, &signature, &self.secret)?;

        serde_json::from_str(payload).map_err(Error::InvalidWebhookJson)
    }

    pub fn verify_headers(
        &self,
        payload: &str,
        headers: &BTreeMap<String, String>,
    ) -> Result<serde_json::Value> {
        let normalized = headers
            .iter()
            .map(|(key, value)| (key.to_ascii_lowercase(), value.as_str()))
            .collect::<BTreeMap<_, _>>();
        let signature = normalized
            .get(SIGNATURE_HEADER)
            .ok_or(Error::InvalidSignatureFormat)?;
        let delivery_timestamp = normalized
            .get(DELIVERY_HEADER)
            .ok_or(Error::InvalidSignatureFormat)?
            .parse::<i64>()
            .map_err(|_| Error::InvalidSignatureFormat)?;

        self.verify(payload, signature, Some(delivery_timestamp))
    }
}

fn parse_signature(signature: &str) -> Result<(i64, String)> {
    let mut timestamp = None;
    let mut v1 = None;

    for part in signature.split(',') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key.trim() {
            "t" => {
                timestamp = Some(
                    value
                        .trim()
                        .parse::<i64>()
                        .map_err(|_| Error::InvalidSignatureFormat)?,
                );
            }
            "v1" => v1 = Some(value.trim().to_string()),
            _ => {}
        }
    }

    match (timestamp, v1) {
        (Some(timestamp), Some(v1)) => Ok((timestamp, v1)),
        _ => Err(Error::InvalidSignatureFormat),
    }
}

fn validate_timestamp(timestamp: i64, tolerance_seconds: i64) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::InvalidSignatureFormat)?
        .as_secs() as i64;
    let difference_seconds = (now - timestamp).abs();

    if difference_seconds <= tolerance_seconds {
        return Ok(());
    }

    Err(Error::TimestampOutsideTolerance {
        difference_seconds,
        tolerance_seconds,
    })
}

fn verify_signature(payload: &str, timestamp: i64, signature: &str, secret: &str) -> Result<()> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| Error::InvalidHmacKey)?;
    mac.update(format!("{timestamp}.{payload}").as_bytes());

    let signature = hex::decode(signature).map_err(|_| Error::InvalidSignature)?;
    mac.verify_slice(&signature)
        .map_err(|_| Error::InvalidSignature)
}
