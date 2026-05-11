use hmac::{Hmac, Mac};
use lettermint::{Error, Webhook};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

fn signature(payload: &str, secret: &str, timestamp: i64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{timestamp}.{payload}").as_bytes());
    format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}

#[test]
fn verifies_valid_webhook_signature() {
    let payload = r#"{"event":"message.delivered","data":{"message_id":"msg_123"}}"#;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let signature = signature(payload, "whsec_test", timestamp);

    let event = Webhook::new("whsec_test")
        .verify(payload, &signature, Some(timestamp))
        .unwrap();

    assert_eq!(event["event"], "message.delivered");
    assert_eq!(event["data"]["message_id"], "msg_123");
}

#[test]
fn rejects_invalid_signature() {
    let payload = r#"{"event":"message.delivered"}"#;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let error = Webhook::new("whsec_test")
        .verify(payload, &format!("t={timestamp},v1=bad"), None)
        .unwrap_err();

    assert!(matches!(error, Error::InvalidSignature));
}

#[test]
fn rejects_timestamps_outside_tolerance() {
    let payload = r#"{"event":"message.delivered"}"#;
    let old_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 600;
    let signature = signature(payload, "whsec_test", old_timestamp);

    let error = Webhook::new("whsec_test")
        .with_tolerance(300)
        .verify(payload, &signature, None)
        .unwrap_err();

    assert!(matches!(error, Error::TimestampOutsideTolerance { .. }));
}
