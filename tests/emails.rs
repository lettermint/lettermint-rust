mod common;

use common::*;
use lettermint::types::{
    MessageTagInput, SandboxResult, SendMailRequest, SendMailRequestSettings, TlsPolicy,
};
use lettermint::{Attachment, Error, SendOptions};
use serde_json::json;

fn minimal() -> SendMailRequest {
    SendMailRequest {
        from: "hello@acme.com".into(),
        to: vec!["jane@example.com".into()],
        subject: "Hi".into(),
        text: Some(Some("Hello".into())),
        ..Default::default()
    }
}

#[tokio::test]
async fn send_posts_exactly_the_message() {
    let transport = MockTransport::new();
    let emails = sending_client(&transport).emails();
    let response = emails.send(&minimal(), SendOptions::new()).await.unwrap();
    assert_eq!(response.message_id, "msg_1");
    assert_eq!(response.status, "pending");

    let request = transport.last();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(target(&request), "/send");
    assert_eq!(header(&request, "content-type"), Some("application/json"));
    assert_eq!(header(&request, "accept"), Some("application/json"));
    assert!(
        header(&request, "user-agent")
            .unwrap()
            .starts_with("lettermint-rust/")
    );
    assert_eq!(header(&request, "idempotency-key"), None);
    assert_eq!(
        body_json(&request),
        json!({"from": "hello@acme.com", "to": ["jane@example.com"], "subject": "Hi", "text": "Hello"})
    );
}

#[tokio::test]
async fn the_builder_sets_every_field() {
    let transport = MockTransport::new();
    let emails = sending_client(&transport).emails();
    emails
        .compose()
        .from("Acme <hello@acme.com>")
        .to(["jane@example.com", "john@example.com"])
        .cc("cc@example.com")
        .bcc(vec!["bcc@example.com".to_owned()])
        .reply_to("support@acme.com")
        .subject("Your invoice")
        .html("<p>Invoice</p>")
        .text("Invoice")
        .headers([("X-Campaign", "invoices")])
        .metadata([("order_id", "1234")])
        .tag("billing")
        .tags([("campaign", "invoices"), ("plan", "pro")])
        .route("transactional")
        .scheduled_at("2026-10-20T09:00:00Z")
        .settings(SendMailRequestSettings {
            track_opens: Some(false),
            tls: Some(TlsPolicy::Enforced),
            ..Default::default()
        })
        .sandbox_result(SandboxResult::HardBounced)
        .attach(Attachment::from_bytes("invoice.pdf", b"%PDF").content_type("application/pdf"))
        .attach(Attachment::from_base64("logo.png", "iVBORw0KGgo=").content_id("logo"))
        .send(SendOptions::new().idempotency_key("invoice-1234"))
        .await
        .unwrap();
    let request = transport.last();
    assert_eq!(header(&request, "idempotency-key"), Some("invoice-1234"));
    assert_eq!(
        body_json(&request),
        json!({
            "route": "transactional",
            "from": "Acme <hello@acme.com>",
            "to": ["jane@example.com", "john@example.com"],
            "cc": ["cc@example.com"],
            "bcc": ["bcc@example.com"],
            "reply_to": ["support@acme.com"],
            "subject": "Your invoice",
            "scheduled_at": "2026-10-20T09:00:00Z",
            "sandbox_result": "hard_bounced",
            "headers": {"X-Campaign": "invoices"},
            "metadata": {"order_id": "1234"},
            "tag": "billing",
            "tags": [{"name": "campaign", "value": "invoices"}, {"name": "plan", "value": "pro"}],
            "settings": {"track_opens": false, "tls": "enforced"},
            "html": "<p>Invoice</p>",
            "text": "Invoice",
            "attachments": [
                {"filename": "invoice.pdf", "content": "JVBERg==", "content_type": "application/pdf"},
                {"filename": "logo.png", "content": "iVBORw0KGgo=", "content_id": "logo"}
            ]
        })
    );
}

#[tokio::test]
async fn a_builder_is_a_reusable_template() {
    let transport = MockTransport::new();
    let emails = sending_client(&transport).emails();
    let base = emails
        .compose()
        .from("hello@acme.com")
        .subject("Welcome")
        .tags([("campaign", "welcome")]);
    let jane = base.clone().to("jane@example.com").html("<p>Jane</p>");
    jane.send(SendOptions::new().idempotency_key("welcome-jane"))
        .await
        .unwrap();
    base.clone()
        .to("john@example.com")
        .send(SendOptions::new())
        .await
        .unwrap();
    // The same builder can be sent again; the key is per call.
    jane.send(SendOptions::new()).await.unwrap();

    let requests = transport.requests();
    assert_eq!(body_json(&requests[0])["to"], json!(["jane@example.com"]));
    assert_eq!(body_json(&requests[0])["html"], json!("<p>Jane</p>"));
    assert_eq!(
        header(&requests[0], "idempotency-key"),
        Some("welcome-jane")
    );
    assert_eq!(body_json(&requests[1])["to"], json!(["john@example.com"]));
    assert!(body_json(&requests[1]).get("html").is_none());
    assert_eq!(header(&requests[1], "idempotency-key"), None);
    assert_eq!(body_json(&requests[2]), body_json(&requests[0]));
    assert_eq!(header(&requests[2], "idempotency-key"), None);
    assert!(base.message().to.is_empty());
}

#[tokio::test]
async fn setters_replace_lists_and_clear_fields() {
    let transport = MockTransport::new();
    let message = sending_client(&transport)
        .emails()
        .compose()
        .to("a@example.com")
        .to(["b@example.com", "c@example.com"])
        .html("x")
        .clear_html()
        .text("y")
        .clear_text()
        .tag("t")
        .clear_tag()
        .scheduled_at("tomorrow")
        .clear_scheduled_at()
        .build()
        .unwrap();
    assert_eq!(message.to, ["b@example.com", "c@example.com"]);
    let json = serde_json::to_value(&message).unwrap();
    for field in ["html", "text", "tag", "scheduled_at"] {
        assert!(json.get(field).is_none(), "{field}: {json}");
    }
}

#[tokio::test]
async fn invalid_tags_fail_before_any_request() {
    let transport = MockTransport::new();
    let emails = sending_client(&transport).emails();
    let builder = emails
        .compose()
        .from("hello@acme.com")
        .to("jane@example.com")
        .subject("Hi")
        .tags([("not a valid tag name!", "x")]);
    let error = builder.send(SendOptions::new()).await.unwrap_err();
    let Error::InvalidInput(input) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(input.field(), "tags");
    assert!(builder.build().is_err());

    let mut message = minimal();
    message.tags = Some(
        (0..21)
            .map(|i| MessageTagInput::from((format!("t{i}"), "v")))
            .collect(),
    );
    assert!(matches!(
        emails.send(&message, SendOptions::new()).await,
        Err(Error::InvalidInput(_))
    ));
    let error = emails
        .send_batch(&[minimal(), message], SendOptions::new())
        .await
        .unwrap_err();
    let Error::InvalidInput(input) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(input.field(), "messages[1].tags");
    assert_eq!(transport.count(), 0);
}

#[tokio::test]
async fn a_failed_send_leaves_nothing_for_the_next_one() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(500, json!({"message": "Server Error"})));
    let emails = sending_client(&transport).emails();
    let failed = emails
        .compose()
        .from("a@example.com")
        .to("first@example.com")
        .subject("First")
        .attach(Attachment::from_bytes("a.txt", "A"))
        .send(SendOptions::new().idempotency_key("first"))
        .await
        .unwrap_err();
    assert!(matches!(failed, Error::Server(_)));
    emails
        .compose()
        .from("b@example.com")
        .to("second@example.com")
        .subject("Second")
        .send(SendOptions::new())
        .await
        .unwrap();
    let second = transport.last();
    assert_eq!(header(&second, "idempotency-key"), None);
    assert_eq!(
        body_json(&second),
        json!({"from": "b@example.com", "to": ["second@example.com"], "subject": "Second"})
    );
}

#[tokio::test]
async fn concurrent_builds_are_isolated() {
    let transport = MockTransport::new();
    let emails = sending_client(&transport).emails();
    let tasks: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|name| {
            let emails = emails.clone();
            tokio::spawn(async move {
                let mut builder = emails.compose();
                builder = builder.from(format!("{name}@acme.com"));
                tokio::task::yield_now().await;
                builder = builder.to(format!("{name}@example.com"));
                tokio::task::yield_now().await;
                builder = builder.subject(name);
                tokio::task::yield_now().await;
                builder.send(SendOptions::new().idempotency_key(name)).await
            })
        })
        .collect();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    for request in transport.requests() {
        let body = body_json(&request);
        let name = body["subject"].as_str().unwrap().to_owned();
        assert_eq!(body["from"], json!(format!("{name}@acme.com")));
        assert_eq!(body["to"], json!([format!("{name}@example.com")]));
        assert_eq!(header(&request, "idempotency-key"), Some(name.as_str()));
    }
}

#[tokio::test]
async fn batches_are_posted_as_an_array() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        202,
        json!([{"message_id": "m1", "status": "pending"}, {"message_id": "m2", "status": "scheduled", "scheduled_at": "2026-10-20T09:00:00Z"}]),
    ));
    let emails = sending_client(&transport).emails();
    let built = emails
        .compose()
        .from("a@example.com")
        .to("b@example.com")
        .subject("Built")
        .build()
        .unwrap();
    let responses = emails
        .send_batch(
            &[minimal(), built],
            SendOptions::new().idempotency_key("batch-1"),
        )
        .await
        .unwrap();
    assert_eq!(responses.len(), 2);
    assert_eq!(
        responses[1].scheduled_at.as_deref(),
        Some("2026-10-20T09:00:00Z")
    );
    let request = transport.last();
    assert_eq!(target(&request), "/send/batch");
    assert_eq!(header(&request, "idempotency-key"), Some("batch-1"));
    assert_eq!(body_json(&request)[1]["subject"], "Built");
}

#[tokio::test]
async fn idempotency_keys_are_checked() {
    let transport = MockTransport::new();
    let error = sending_client(&transport)
        .emails()
        .send(
            &minimal(),
            SendOptions::new().idempotency_key("a\r\nX-Injected: 1"),
        )
        .await
        .unwrap_err();
    let Error::InvalidInput(input) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(input.field(), "idempotency_key");
    assert_eq!(transport.count(), 0);
}

#[tokio::test]
async fn a_scheduled_send_and_sandbox_fields_decode() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        202,
        json!({"message_id": "m1", "status": "scheduled", "scheduled_at": "2026-10-20T09:00:00Z", "sandbox": true, "sandbox_result": "delivered"}),
    ));
    let response = sending_client(&transport)
        .emails()
        .send(&minimal(), SendOptions::new())
        .await
        .unwrap();
    assert_eq!(response.status, lettermint::types::MessageStatus::Scheduled);
    assert_eq!(response.sandbox, Some(true));
    assert_eq!(response.sandbox_result, Some(SandboxResult::Delivered));
}
