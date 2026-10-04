mod common;

use common::*;
use futures_core::Stream;
use lettermint::types::{
    GetDomainQueryIncludeItem, GetStatsQuery, ListDomainsQuery, ListDomainsQuerySortItem,
    ListWebhookDeliveriesQuery, MessageStatus, RescheduleMessageRequest, StoreDomainData,
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
