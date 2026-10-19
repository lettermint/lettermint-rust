mod common;

use common::*;
use futures_core::Stream;
use lettermint::types::{
    GetDomainQueryIncludeItem, GetStatsQuery, ListDomainsQuery, ListDomainsQuerySortItem,
    ListWebhookDeliveriesQuery, MessageStatus, RescheduleMessageRequest, ResendMessageRequest,
    StoreDomainData,
};
use lettermint::{Error, SendOptions};
use serde_json::{Value, json};

fn page(items: Value, next_cursor: Option<&str>) -> Reply {
    Reply::json(
        200,
        json!({"data": items, "path": "https://api.example.test/v1/domains", "per_page": 2, "next_cursor": next_cursor, "next_page_url": null, "prev_cursor": null, "prev_page_url": null}),
    )
}

fn domain(id: &str) -> Value {
    json!({"id": id, "domain": format!("{id}.example.com"), "status": "verified", "dkim_mode": "managed_cname", "rotation_ready": true, "status_changed_at": null, "created_at": "2026-10-01T00:00:00Z"})
}

#[tokio::test]
async fn list_sends_typed_query_parameters() {
    let transport = MockTransport::new();
    transport.reply(page(json!([domain("d1")]), None));
    let client = team_client(&transport);
    let page = client
        .domains()
        .list(&ListDomainsQuery {
            page_size: Some(30),
            sort: Some(vec![ListDomainsQuerySortItem::CreatedAtDesc]),
            filter_status: Some("verified".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.data[0].domain, "d1.example.com");
    assert_eq!(page.per_page, 2);
    assert_eq!(page.next_cursor, None);
    let request = transport.last();
    assert_eq!(request.method, http::Method::GET);
    assert_eq!(
        target(&request),
        "/domains?page%5Bsize%5D=30&sort=-created_at&filter%5Bstatus%5D=verified"
    );
    assert_eq!(request.body, None);
    assert_eq!(header(&request, "content-type"), None);
}

#[tokio::test]
async fn path_parameters_are_encoded_and_checked() {
    let transport = MockTransport::new();
    transport.always(Reply::json(200, json!({"message": "ok"})));
    let client = team_client(&transport);
    client
        .domains()
        .verify_dns_record("a/b c", "r?1")
        .await
        .unwrap();
    let request = transport.last();
    assert_eq!(
        target(&request),
        "/domains/a%2Fb%20c/dns-records/r%3F1/verify"
    );
    assert_eq!(request.method, http::Method::POST);
    // A POST without a request body sends no body and no content type.
    assert_eq!(request.body, None);
    assert_eq!(header(&request, "content-type"), None);

    let before = transport.count();
    for id in ["", ".", ".."] {
        let error = client.domains().delete(id).await.unwrap_err();
        assert!(matches!(error, Error::Config(_)), "{id:?}: {error:?}");
        assert!(error.to_string().contains("`domain_id`"), "{error}");
    }
    assert_eq!(transport.count(), before);
}

#[tokio::test]
async fn bodies_queries_and_text_responses() {
    let transport = MockTransport::new();
    let client = team_client(&transport);

    transport.reply(Reply::json(201, domain("d2")));
    client
        .domains()
        .create(&StoreDomainData {
            domain: "d2.example.com".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        body_json(&transport.last()),
        json!({"domain": "d2.example.com"})
    );

    transport.reply(Reply::json(200, domain("d2")));
    client
        .domains()
        .retrieve(
            "d2",
            &lettermint::types::GetDomainQuery {
                include: Some(vec![
                    GetDomainQueryIncludeItem::DnsRecords,
                    GetDomainQueryIncludeItem::Projects,
                ]),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        target(&transport.last()),
        "/domains/d2?include=dnsRecords%2Cprojects"
    );

    transport.reply(Reply::text(200, "text/html; charset=UTF-8", "<p>Hi</p>"));
    assert_eq!(client.messages().html("m1").await.unwrap(), "<p>Hi</p>");
    transport.reply(Reply::text(
        200,
        "message/rfc822",
        "Subject: Hi\r\n\r\nBody",
    ));
    assert_eq!(
        client.messages().source("m1").await.unwrap(),
        "Subject: Hi\r\n\r\nBody"
    );

    transport.reply(Reply::json(
        200,
        json!({"message_id": "m1", "status": "scheduled", "scheduled_at": "2026-10-21T09:00:00Z"}),
    ));
    let scheduled = client
        .messages()
        .reschedule(
            "m1",
            &RescheduleMessageRequest {
                scheduled_at: "2026-10-21T09:00:00Z".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(scheduled.status, Some(MessageStatus::Scheduled));
    let request = transport.last();
    assert_eq!(request.method, http::Method::PATCH);
    assert_eq!(
        body_json(&request),
        json!({"scheduled_at": "2026-10-21T09:00:00Z"})
    );

    transport.reply(Reply::json(200, json!({"daily": [], "totals": {}})));
    let _ = client
        .stats()
        .retrieve(&GetStatsQuery {
            from: "2026-10-01".into(),
            to: "2026-10-31".into(),
            project_id: None,
            include_machine: Some(true),
        })
        .await;
    assert_eq!(
        target(&transport.last()),
        "/stats?from=2026-10-01&to=2026-10-31&include_machine=1"
    );
}

#[tokio::test]
async fn delete_with_204_returns_unit() {
    let transport = MockTransport::new();
    transport.reply(Reply::empty(204));
    team_client(&transport)
        .projects()
        .report_forwarding()
        .delete("p1")
        .await
        .unwrap();
    let request = transport.last();
    assert_eq!(request.method, http::Method::DELETE);
    assert_eq!(target(&request), "/projects/p1/report-forwarding");
}

#[tokio::test]
async fn process_takes_an_idempotency_key_and_sends_no_body() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        202,
        json!({"data": {"message_id": "m1", "status": "processing"}}),
    ));
    let _ = team_client(&transport)
        .messages()
        .process("m1", SendOptions::new().idempotency_key("process-m1"))
        .await;
    let request = transport.last();
    assert_eq!(target(&request), "/messages/m1/process");
    assert_eq!(request.body, None);
    assert_eq!(header(&request, "idempotency-key"), Some("process-m1"));
}

#[tokio::test]
async fn resend_posts_the_recipients_with_an_idempotency_key() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        202,
        json!({"message_id": "m2", "status": "pending"}),
    ));
    let pending = team_client(&transport)
        .messages()
        .resend(
            "m1",
            &ResendMessageRequest {
                recipients: Some(vec!["jane@example.com".into()]),
            },
            SendOptions::new().idempotency_key("resend-m1"),
        )
        .await
        .unwrap();
    assert_eq!(pending.message_id, "m2");
    assert_eq!(pending.status, "pending");
    let request = transport.last();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(target(&request), "/messages/m1/resend");
    assert_eq!(
        header(&request, "authorization"),
        Some(format!("Bearer {TEAM_TOKEN}").as_str())
    );
    assert_eq!(header(&request, "idempotency-key"), Some("resend-m1"));
    assert_eq!(
        body_json(&request),
        json!({"recipients": ["jane@example.com"]})
    );
}

#[tokio::test]
async fn resend_to_every_recipient_sends_an_empty_object() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        202,
        json!({"message_id": "m2", "status": "pending"}),
    ));
    // A sending-only client can resend the email it sent.
    sending_client(&transport)
        .messages()
        .resend("m1", &Default::default(), SendOptions::new())
        .await
        .unwrap();
    let request = transport.last();
    assert_eq!(target(&request), "/messages/m1/resend");
    assert_eq!(body_json(&request), json!({}));
    assert_eq!(header(&request, "content-type"), Some("application/json"));
    assert_eq!(header(&request, "x-lettermint-token"), Some(SENDING_TOKEN));
    assert_eq!(header(&request, "idempotency-key"), None);

    let error = sending_client(&transport)
        .messages()
        .resend("..", &Default::default(), SendOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Config(_)), "{error:?}");
    assert_eq!(transport.count(), 1);
}

#[tokio::test]
async fn resend_after_the_content_expired_is_an_api_error() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        410,
        json!({"error": {"code": "MESSAGE_CONTENT_UNAVAILABLE", "message": "The content of this message is no longer available."}}),
    ));
    let error = team_client(&transport)
        .messages()
        .resend("m1", &Default::default(), SendOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Api(_)), "{error:?}");
    assert_eq!(error.status(), Some(410));
    assert_eq!(error.code(), Some("MESSAGE_CONTENT_UNAVAILABLE"));
    assert_eq!(
        error.api_error().unwrap().message(),
        "The content of this message is no longer available."
    );
}

#[tokio::test]
async fn iterate_follows_cursors() {
    let transport = MockTransport::new();
    transport.reply(page(json!([domain("d1"), domain("d2")]), Some("c2")));
    transport.reply(page(json!([domain("d3")]), Some("c3")));
    transport.reply(page(json!([]), None));
    let client = team_client(&transport);
    let mut domains = client.domains().iterate(&ListDomainsQuery {
        page_size: Some(2),
        ..Default::default()
    });
    let mut ids = Vec::new();
    while let Some(domain) = domains.next().await {
        ids.push(domain.unwrap().id);
    }
    assert_eq!(ids, ["d1", "d2", "d3"]);
    let targets: Vec<String> = transport
        .requests()
        .iter()
        .map(|r| target(r).to_owned())
        .collect();
    assert_eq!(
        targets,
        [
            "/domains?page%5Bsize%5D=2",
            "/domains?page%5Bsize%5D=2&page%5Bcursor%5D=c2",
            "/domains?page%5Bsize%5D=2&page%5Bcursor%5D=c3"
        ]
    );
}

#[tokio::test]
async fn iterate_stops_on_a_repeated_cursor_and_requests_lazily() {
    let transport = MockTransport::new();
    transport.reply(page(json!([domain("d1")]), Some("same")));
    transport.reply(page(json!([domain("d2")]), Some("same")));
    let client = team_client(&transport);
    let mut domains = client.domains().iterate(&Default::default());
    assert_eq!(transport.count(), 0);
    assert_eq!(domains.next().await.unwrap().unwrap().id, "d1");
    assert_eq!(transport.count(), 1);
    assert_eq!(domains.next().await.unwrap().unwrap().id, "d2");
    assert!(domains.next().await.is_none());
    assert_eq!(transport.count(), 2);
}

#[tokio::test]
async fn webhook_lists_use_the_cursor_parameter() {
    let transport = MockTransport::new();
    let delivery = |id: &str| json!({"id": id, "webhook_id": "w1", "event_type": "message.delivered", "source_scope": "team", "source_project_id": null, "source_route_id": null, "status": "success", "sandbox": false, "attempt_number": 1, "http_status_code": 200, "duration_ms": 12, "delivered_at": null, "created_at": "2026-10-01T00:00:00Z"});
    transport.reply(page(json!([delivery("x1")]), Some("n2")));
    transport.reply(page(json!([delivery("x2")]), None));
    let client = team_client(&transport);
    let mut deliveries = client
        .webhooks()
        .deliveries()
        .iterate("w1", &ListWebhookDeliveriesQuery::default());
    let mut seen = 0;
    while let Some(item) = deliveries.next().await {
        item.unwrap();
        seen += 1;
    }
    assert_eq!(seen, 2);
    assert_eq!(
        target(&transport.last()),
        "/webhooks/w1/deliveries?cursor=n2"
    );
}

fn delivery_detail(next_retry_at: Option<Value>) -> Value {
    let mut delivery = json!({"id": "x1", "webhook_id": "w1", "event_type": "message.delivered", "source_scope": "team", "source_project_id": null, "source_route_id": null, "status": "failed", "sandbox": false, "attempt_number": 2, "http_status_code": 500, "duration_ms": 12, "payload": [], "response_body": "boom", "response_headers": null, "error_message": "HTTP 500", "delivered_at": null, "timestamp": "2026-10-01T00:00:00Z"});
    if let Some(value) = next_retry_at {
        delivery["next_retry_at"] = value;
    }
    delivery
}

#[tokio::test]
async fn deliveries_expose_the_next_automatic_retry() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        200,
        delivery_detail(Some(json!("2026-10-01T00:05:00Z"))),
    ));
    transport.reply(Reply::json(200, delivery_detail(Some(Value::Null))));
    transport.reply(Reply::json(200, delivery_detail(None)));
    let client = team_client(&transport);
    let deliveries = client.webhooks().deliveries();

    let scheduled = deliveries.retrieve("w1", "x1").await.unwrap();
    assert_eq!(
        scheduled.next_retry_at,
        Some(Some("2026-10-01T00:05:00Z".into()))
    );
    assert_eq!(target(&transport.last()), "/webhooks/w1/deliveries/x1");
    // null: no automatic retry is scheduled.
    let none = deliveries.retrieve("w1", "x1").await.unwrap();
    assert_eq!(none.next_retry_at, Some(None));
    let absent = deliveries.retrieve("w1", "x1").await.unwrap();
    assert_eq!(absent.next_retry_at, None);
}

#[tokio::test]
async fn retry_delivery_posts_without_a_body() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        202,
        json!({"message": "Webhook delivery retry queued.", "delivery_id": "x1"}),
    ));
    let retried = team_client(&transport)
        .webhooks()
        .deliveries()
        .retry("w1", "x1")
        .await
        .unwrap();
    assert_eq!(retried.message, "Webhook delivery retry queued.");
    assert_eq!(retried.delivery_id, "x1");
    let request = transport.last();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(target(&request), "/webhooks/w1/deliveries/x1/retry");
    assert_eq!(
        header(&request, "authorization"),
        Some(format!("Bearer {TEAM_TOKEN}").as_str())
    );
    assert_eq!(header(&request, "x-lettermint-token"), None);
    assert_eq!(header(&request, "idempotency-key"), None);
    assert_eq!(request.body, None);
    assert_eq!(header(&request, "content-type"), None);
}

#[tokio::test]
async fn retry_delivery_checks_its_path_parameters_and_token() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    for (webhook_id, delivery_id, name) in
        [("..", "x1", "`webhook_id`"), ("w1", "", "`delivery_id`")]
    {
        let error = client
            .webhooks()
            .deliveries()
            .retry(webhook_id, delivery_id)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Config(_)), "{error:?}");
        assert!(error.to_string().contains(name), "{error}");
    }

    let error = sending_client(&transport)
        .webhooks()
        .deliveries()
        .retry("w1", "x1")
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Config(_)), "{error:?}");
    assert!(error.to_string().contains("team_token"), "{error}");
    assert_eq!(transport.count(), 0);
}

#[tokio::test]
async fn retry_delivery_conflicts_expose_the_code() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        409,
        json!({"error": {"code": "WEBHOOK_DELIVERY_NOT_RETRYABLE", "message": "This delivery cannot be retried."}}),
    ));
    let error = team_client(&transport)
        .webhooks()
        .deliveries()
        .retry("w1", "x1")
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Conflict(_)), "{error:?}");
    assert_eq!(error.status(), Some(409));
    assert_eq!(error.code(), Some("WEBHOOK_DELIVERY_NOT_RETRYABLE"));
    assert_eq!(
        error.api_error().unwrap().message(),
        "This delivery cannot be retried."
    );
}

#[tokio::test]
async fn retry_delivery_rate_limits_accept_a_plain_message() {
    let transport = MockTransport::new();
    transport.reply(
        Reply::json(429, json!({"message": "Too Many Attempts."})).header("retry-after", "30"),
    );
    let error = team_client(&transport)
        .webhooks()
        .deliveries()
        .retry("w1", "x1")
        .await
        .unwrap_err();
    assert!(matches!(error, Error::RateLimit(_)), "{error:?}");
    assert_eq!(error.code(), None);
    assert_eq!(error.api_error().unwrap().message(), "Too Many Attempts.");
    assert_eq!(
        error.api_error().unwrap().retry_after(),
        Some(std::time::Duration::from_secs(30))
    );
}

#[tokio::test]
async fn iterate_is_a_stream_and_ends_after_an_error() {
    let transport = MockTransport::new();
    transport.reply(page(json!([domain("d1")]), Some("c2")));
    transport.reply(Reply::json(500, json!({"message": "boom"})));
    let client = team_client(&transport);
    let mut stream = client.domains().iterate(&Default::default());
    let mut results = Vec::new();
    while let Some(item) =
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut stream).poll_next(cx)).await
    {
        results.push(item.map(|domain| domain.id));
    }
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].as_deref().unwrap(), "d1");
    assert!(matches!(results[1], Err(Error::Server(_))));
    assert_eq!(transport.count(), 2);
}

#[tokio::test]
async fn iterate_reports_config_errors_on_first_use() {
    let transport = MockTransport::new();
    let client = sending_client(&transport);
    let mut routes = client.routes().iterate("..", &Default::default());
    assert!(matches!(routes.next().await, Some(Err(Error::Config(_)))));
    assert!(routes.next().await.is_none());
    assert_eq!(transport.count(), 0);
}
