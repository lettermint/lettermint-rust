mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::*;
use lettermint::types::MessageStatus;
use lettermint::{Error, Lettermint, SendOptions};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn message() -> lettermint::types::SendMailRequest {
    lettermint::types::SendMailRequest {
        from: "a@example.com".into(),
        to: vec!["b@example.com".into()],
        subject: "Hi".into(),
        ..Default::default()
    }
}

async fn send(
    transport: &MockTransport,
) -> lettermint::Result<lettermint::types::SendMailResponse> {
    sending_client(transport)
        .emails()
        .send(&message(), SendOptions::new())
        .await
}

#[tokio::test]
async fn unknown_enum_values_decode_and_round_trip() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        202,
        json!({"message_id": "m1", "status": "some_future_status"}),
    ));
    let response = send(&transport).await.unwrap();
    assert_eq!(
        response.status,
        MessageStatus::Other("some_future_status".into())
    );
    assert_eq!(response.status.as_str(), "some_future_status");
    assert!(!response.status.is_known());
    assert_eq!(
        serde_json::to_value(&response).unwrap(),
        json!({"message_id": "m1", "status": "some_future_status"})
    );
}

#[tokio::test]
async fn unknown_fields_are_ignored() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        202,
        json!({"message_id": "m1", "status": "pending", "some_future_field": {"nested": [1, 2, 3]}}),
    ));
    assert_eq!(send(&transport).await.unwrap().message_id, "m1");
}

#[tokio::test]
async fn a_missing_required_field_is_an_unexpected_response() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(202, json!({"status": "pending"})));
    let error = send(&transport).await.unwrap_err();
    let Error::UnexpectedResponse(unexpected) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(unexpected.status(), 202);
    assert!(unexpected.message().contains("message_id"), "{error}");
}

#[tokio::test]
async fn empty_and_non_json_bodies_are_unexpected_responses() {
    let transport = MockTransport::new();
    transport.reply(Reply::text(202, "application/json", ""));
    let error = send(&transport).await.unwrap_err();
    assert!(matches!(error, Error::UnexpectedResponse(_)), "{error:?}");
    assert_eq!(error.status(), Some(202));

    transport.reply(Reply::text(200, "application/json", "{not json"));
    assert_eq!(send(&transport).await.unwrap_err().status(), Some(200));

    let html = "<html><head><title>502 Bad Gateway</title></head><body><h1>502 Bad Gateway</h1></body></html>";
    transport.reply(Reply::text(502, "text/html; charset=UTF-8", html));
    let error = send(&transport).await.unwrap_err();
    let Error::UnexpectedResponse(unexpected) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(unexpected.status(), 502);
    assert!(unexpected.message().contains("(text/html)"), "{error}");
    assert_eq!(unexpected.body_excerpt(), html);

    let long = "x".repeat(500);
    transport.reply(Reply::text(503, "text/plain", &long));
    let error = send(&transport).await.unwrap_err();
    let Error::UnexpectedResponse(unexpected) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(unexpected.body_excerpt().chars().count(), 201);
}

#[tokio::test]
async fn error_statuses_are_typed() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        422,
        json!({"message": "The to field is required.", "errors": {"to": ["The to field is required."]}}),
    ));
    let error = send(&transport).await.unwrap_err();
    let Error::Validation(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api.status(), 422);
    assert_eq!(api.errors().unwrap()["to"][0], "The to field is required.");

    transport.reply(Reply::empty(401));
    assert!(matches!(
        send(&transport).await.unwrap_err(),
        Error::Authentication(_)
    ));
    transport.reply(Reply::json(429, json!({"message": "Slow down"})).header("retry-after", "3"));
    let error = send(&transport).await.unwrap_err();
    let Error::RateLimit(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api.retry_after(), Some(Duration::from_secs(3)));
    assert_eq!(api.message(), "Slow down");
    transport.reply(Reply::json(
        400,
        json!({"error": {"code": "BAD", "message": "Bad request"}}),
    ));
    let error = send(&transport).await.unwrap_err();
    assert!(matches!(error, Error::Api(_)));
    assert_eq!(error.code(), Some("BAD"));
}

#[tokio::test]
async fn tokens_are_removed_from_error_bodies_and_messages() {
    let transport = MockTransport::new();
    let echo = json!({"message": format!("bad token {SENDING_TOKEN}"), "errors": {"token": [SENDING_TOKEN]}, "nested": {"list": [format!("x{SENDING_TOKEN}x")]}});
    for status in [401, 422, 500] {
        transport.reply(Reply::json(status, echo.clone()));
        let error = send(&transport).await.unwrap_err();
        let rendered = format!("{error} {error:?} {:?}", error.api_error().unwrap().body());
        assert!(!rendered.contains(SENDING_TOKEN), "{status}: {rendered}");
        assert!(rendered.contains("[redacted]"));
    }
    transport.reply(Reply::text(
        502,
        "text/html",
        &format!("<p>{SENDING_TOKEN}</p>"),
    ));
    let error = send(&transport).await.unwrap_err();
    assert!(!format!("{error:?}").contains(SENDING_TOKEN));
}

#[tokio::test]
async fn redirects_are_errors() {
    let transport = MockTransport::new();
    transport.reply(
        Reply::json(307, json!({"message": "Moved"}))
            .header("location", "https://elsewhere.test/v1/send"),
    );
    let error = send(&transport).await.unwrap_err();
    assert!(
        matches!(error, Error::Redirect { status: 307, .. }),
        "{error:?}"
    );
    assert_eq!(error.status(), Some(307));
    assert_eq!(transport.count(), 1);
}

#[tokio::test]
async fn the_timeout_covers_a_slow_transport() {
    let transport = MockTransport::new();
    transport.reply(
        Reply::json(202, json!({"message_id": "m", "status": "pending"}))
            .delayed(Duration::from_secs(5)),
    );
    let client = sending_client(&transport)
        .with_timeout(Duration::from_millis(100))
        .unwrap();
    let started = Instant::now();
    let error = client
        .emails()
        .send(&message(), SendOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Timeout { .. }), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(error.is_retryable());
    // A per-send timeout overrides the client's.
    transport.reply(
        Reply::json(202, json!({"message_id": "m", "status": "pending"}))
            .delayed(Duration::from_secs(5)),
    );
    let error = sending_client(&transport)
        .emails()
        .send(
            &message(),
            SendOptions::new().timeout(Duration::from_millis(50)),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("50 ms"), "{error}");
}

#[tokio::test]
async fn dropping_the_future_cancels_the_request() {
    let transport = MockTransport::new();
    transport.reply(
        Reply::json(202, json!({"message_id": "m", "status": "pending"}))
            .delayed(Duration::from_secs(5)),
    );
    let client = sending_client(&transport);
    let outcome = tokio::time::timeout(
        Duration::from_millis(50),
        client.emails().send(&message(), SendOptions::new()),
    )
    .await;
    assert!(outcome.is_err());
}

// --- The default reqwest transport against local servers -------------------------------------

/// Serves one canned response per connection and counts the connections.
async fn serve(
    response: impl Fn(u16) -> Vec<u8> + Send + Sync + 'static,
    stall: Option<Duration>,
) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buffer = vec![0; 65536];
            let _ = socket.read(&mut buffer).await;
            let _ = socket.write_all(&response(address.port())).await;
            if let Some(stall) = stall {
                tokio::time::sleep(stall).await;
            }
            let _ = socket.shutdown().await;
        }
    });
    (format!("http://{address}"), hits)
}

#[tokio::test]
async fn reqwest_never_follows_redirects() {
    let (foreign, foreign_hits) = serve(
        |_| b"HTTP/1.1 202 Accepted\r\ncontent-type: application/json\r\ncontent-length: 45\r\nconnection: close\r\n\r\n{\"message_id\":\"captured\",\"status\":\"pending\"}".to_vec(),
        None,
    )
    .await;
    let location = format!("{foreign}/v1/send");
    let (api, _) = serve(
        move |_| format!("HTTP/1.1 307 Temporary Redirect\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n").into_bytes(),
        None,
    )
    .await;
    let client = Lettermint::builder()
        .sending_token(SENDING_TOKEN)
        .base_url(format!("{api}/v1"))
        .build()
        .unwrap();
    let error = client
        .emails()
        .send(&message(), SendOptions::new())
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Redirect { status: 307, .. }),
        "{error:?}"
    );
    assert_eq!(foreign_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reqwest_timeout_covers_the_body() {
    // Headers arrive at once; the body stalls.
    let (api, _) = serve(
        |_| b"HTTP/1.1 202 Accepted\r\ncontent-type: application/json\r\ncontent-length: 100\r\n\r\n{\"message_id\":".to_vec(),
        Some(Duration::from_secs(5)),
    )
    .await;
    let client = Lettermint::builder()
        .sending_token(SENDING_TOKEN)
        .base_url(format!("{api}/v1"))
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap();
    let started = Instant::now();
    let error = client
        .emails()
        .send(&message(), SendOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Timeout { .. }), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn reqwest_connection_failures_are_typed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let client = Lettermint::builder()
        .sending_token(SENDING_TOKEN)
        .base_url(format!("http://{address}/v1"))
        .build()
        .unwrap();
    let error = client
        .emails()
        .send(&message(), SendOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Connection { .. }), "{error:?}");
    assert!(!format!("{error:?}").contains(SENDING_TOKEN));
}
