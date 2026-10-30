mod common;

use common::*;
use futures_core::Stream;
use lettermint::types::{
    ApiTokenSource, BuiltInTeamRole, CreateTeamInviteData, CreateTeamInviteDataProjectAccess,
    CreateTeamRoleData, DeleteTeamRoleQuery, DuplicateTeamRoleData, GetDomainQueryIncludeItem,
    GetStatsQuery, ListDomainsQuery, ListDomainsQuerySortItem, ListProjectApiTokensQuery,
    ListProjectApiTokensQuerySortItem, ListTeamInvitesQuery, ListTeamMembersQuery,
    ListWebhookDeliveriesQuery, MessageStatus, ProjectAccessScope, RbacPermission,
    RescheduleMessageRequest, ResendMessageRequest, StoreDomainData, StoreProjectApiTokenData,
    UpdateTeamInviteData, UpdateTeamInviteDataProjectAccess, UpdateTeamRoleData,
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

fn invite(id: &str) -> Value {
    json!({"id": id, "email": format!("{id}@example.com"), "role": {"id": "r1", "name": "Developer"}, "project_access": {"scope": "selected", "projects": [{"id": "p1", "name": "Production"}]}, "created_at": "2026-10-01T00:00:00Z"})
}

fn assert_team_auth(request: &lettermint::HttpRequest) {
    assert_eq!(
        header(request, "authorization"),
        Some(format!("Bearer {TEAM_TOKEN}").as_str())
    );
    assert_eq!(header(request, "x-lettermint-token"), None);
    assert_eq!(header(request, "idempotency-key"), None);
}

#[tokio::test]
async fn invites_list_retrieve_and_iterate() {
    let transport = MockTransport::new();
    let client = team_client(&transport);

    transport.reply(page(json!([invite("i1")]), Some("c2")));
    let page_one = client
        .team()
        .invites()
        .list(&ListTeamInvitesQuery {
            page_size: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page_one.data[0].email, "i1@example.com");
    assert_eq!(page_one.data[0].role.name, "Developer");
    assert_eq!(
        page_one.data[0].project_access.scope,
        ProjectAccessScope::Selected
    );
    assert_eq!(page_one.data[0].project_access.projects[0].id, "p1");
    assert_eq!(page_one.next_cursor.as_deref(), Some("c2"));
    let request = transport.last();
    assert_eq!(request.method, http::Method::GET);
    assert_eq!(target(&request), "/team/invites?page%5Bsize%5D=1");
    assert_team_auth(&request);
    assert_eq!(request.body, None);

    transport.reply(Reply::json(200, invite("i1")));
    let retrieved = client.team().invites().retrieve("i1").await.unwrap();
    assert_eq!(retrieved.id, "i1");
    assert_eq!(retrieved.created_at, "2026-10-01T00:00:00Z");
    let request = transport.last();
    assert_eq!(request.method, http::Method::GET);
    assert_eq!(target(&request), "/team/invites/i1");
    assert_team_auth(&request);

    transport.reply(page(json!([invite("i1"), invite("i2")]), Some("c2")));
    transport.reply(page(json!([invite("i3")]), None));
    let mut invites = client.team().invites().iterate(&ListTeamInvitesQuery {
        page_size: Some(2),
        ..Default::default()
    });
    let mut ids = Vec::new();
    while let Some(invite) = invites.next().await {
        ids.push(invite.unwrap().id);
    }
    assert_eq!(ids, ["i1", "i2", "i3"]);
    assert_eq!(
        target(&transport.last()),
        "/team/invites?page%5Bsize%5D=2&page%5Bcursor%5D=c2"
    );
}

#[tokio::test]
async fn invites_create_update_and_revoke() {
    let transport = MockTransport::new();
    let client = team_client(&transport);

    transport.reply(Reply::json(201, invite("i1")));
    let created = client
        .team()
        .invites()
        .create(&CreateTeamInviteData::new(
            "i1@example.com",
            "r1",
            CreateTeamInviteDataProjectAccess {
                scope: ProjectAccessScope::Selected,
                project_ids: Some(vec!["p1".into()]),
            },
        ))
        .await
        .unwrap();
    assert_eq!(created.id, "i1");
    assert_eq!(created.role.id, "r1");
    let request = transport.last();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(target(&request), "/team/invites");
    assert_team_auth(&request);
    assert_eq!(header(&request, "content-type"), Some("application/json"));
    assert_eq!(
        body_json(&request),
        json!({"email": "i1@example.com", "role_id": "r1", "project_access": {"scope": "selected", "project_ids": ["p1"]}})
    );

    transport.reply(Reply::json(200, invite("i1")));
    let updated = client
        .team()
        .invites()
        .update(
            "i1",
            &UpdateTeamInviteData::new(
                "new@example.com",
                "r2",
                UpdateTeamInviteDataProjectAccess::new(ProjectAccessScope::All),
            ),
        )
        .await
        .unwrap();
    assert_eq!(updated.id, "i1");
    let request = transport.last();
    assert_eq!(request.method, http::Method::PUT);
    assert_eq!(target(&request), "/team/invites/i1");
    assert_team_auth(&request);
    // Without project ids, only the scope is sent.
    assert_eq!(
        body_json(&request),
        json!({"email": "new@example.com", "role_id": "r2", "project_access": {"scope": "all"}})
    );

    transport.reply(Reply::json(200, json!({"message": "Team invite revoked."})));
    let revoked = client.team().invites().revoke("i1").await.unwrap();
    assert_eq!(revoked.message, "Team invite revoked.");
    let request = transport.last();
    assert_eq!(request.method, http::Method::DELETE);
    assert_eq!(target(&request), "/team/invites/i1");
    assert_team_auth(&request);
    assert_eq!(request.body, None);
}

#[tokio::test]
async fn invite_errors_map_to_the_error_variants() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    let body = || {
        CreateTeamInviteData::new(
            "i1@example.com",
            "r1",
            CreateTeamInviteDataProjectAccess::new(ProjectAccessScope::All),
        )
    };

    transport.reply(Reply::json(
        409,
        json!({"message": "The team has no free seat."}),
    ));
    let error = client.team().invites().create(&body()).await.unwrap_err();
    assert!(matches!(error, Error::Conflict(_)), "{error:?}");
    assert_eq!(error.status(), Some(409));
    assert_eq!(
        error.api_error().unwrap().message(),
        "The team has no free seat."
    );

    transport.reply(Reply::json(
        422,
        json!({"message": "The email field must be a valid email address.", "errors": {"email": ["The email field must be a valid email address."]}}),
    ));
    let error = client.team().invites().create(&body()).await.unwrap_err();
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    let errors = error.api_error().unwrap().errors().unwrap();
    assert_eq!(
        errors["email"],
        ["The email field must be a valid email address."]
    );
}

#[tokio::test]
async fn invites_check_their_path_parameters_and_token() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    let invites = client.team().invites();
    let body = UpdateTeamInviteData::new(
        "i1@example.com",
        "r1",
        UpdateTeamInviteDataProjectAccess::new(ProjectAccessScope::All),
    );
    for id in ["", ".", ".."] {
        let errors = [
            invites.retrieve(id).await.unwrap_err(),
            invites.update(id, &body).await.unwrap_err(),
            invites.revoke(id).await.unwrap_err(),
        ];
        for error in errors {
            assert!(matches!(error, Error::Config(_)), "{id:?}: {error:?}");
            assert!(error.to_string().contains("`invite_id`"), "{error}");
        }
    }

    let error = sending_client(&transport)
        .team()
        .invites()
        .list(&Default::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Config(_)), "{error:?}");
    assert!(error.to_string().contains("team_token"), "{error}");
    assert_eq!(transport.count(), 0);
}

fn role(id: &str, description: Option<Value>) -> Value {
    let mut role = json!({"id": id, "name": "Developer", "system_key": null, "permissions": ["members:read"], "assignable": true});
    if let Some(value) = description {
        role["description"] = value;
    }
    role
}

#[tokio::test]
async fn roles_retrieve_with_and_without_a_description() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(200, role("r1", Some(json!("Builds things")))));
    transport.reply(Reply::json(200, role("r1", Some(Value::Null))));
    transport.reply(Reply::json(200, role("r1", None)));
    transport.reply(Reply::json(
        200,
        json!({"id": "r0", "name": "Owner", "description": null, "system_key": "owner", "permissions": [], "assignable": false}),
    ));
    let client = team_client(&transport);
    let team = client.team();

    let described = team.retrieve_role("r1").await.unwrap();
    assert_eq!(described.id, "r1");
    assert_eq!(described.description, Some(Some("Builds things".into())));
    assert_eq!(described.system_key, None);
    assert_eq!(described.permissions, [RbacPermission::MembersRead]);
    assert!(described.assignable);
    let request = transport.last();
    assert_eq!(request.method, http::Method::GET);
    assert_eq!(target(&request), "/team/roles/r1");
    assert_team_auth(&request);
    assert_eq!(request.body, None);

    // null: the role has no description.
    let null = team.retrieve_role("r1").await.unwrap();
    assert_eq!(null.description, Some(None));
    let absent = team.retrieve_role("r1").await.unwrap();
    assert_eq!(absent.description, None);
    let owner = team.retrieve_role("r0").await.unwrap();
    assert_eq!(owner.system_key, Some(BuiltInTeamRole::Owner));
    assert!(!owner.assignable);
}

#[tokio::test]
async fn roles_create_update_and_duplicate_send_their_bodies() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    let team = client.team();

    transport.reply(Reply::json(201, role("r2", Some(json!("Reads members")))));
    let created = team
        .create_role(&CreateTeamRoleData {
            name: "Developer".into(),
            description: Some(Some("Reads members".into())),
            permissions: vec![RbacPermission::MembersRead],
        })
        .await
        .unwrap();
    assert_eq!(created.id, "r2");
    assert_eq!(created.description, Some(Some("Reads members".into())));
    let request = transport.last();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(target(&request), "/team/roles");
    assert_team_auth(&request);
    assert_eq!(header(&request, "content-type"), Some("application/json"));
    assert_eq!(
        body_json(&request),
        json!({"name": "Developer", "description": "Reads members", "permissions": ["members:read"]})
    );

    // Without a description the key is left out.
    transport.reply(Reply::json(201, role("r3", Some(Value::Null))));
    team.create_role(&CreateTeamRoleData {
        name: "Viewer".into(),
        permissions: vec![RbacPermission::MembersRead],
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(
        body_json(&transport.last()),
        json!({"name": "Viewer", "permissions": ["members:read"]})
    );

    // Some(None) clears the description.
    transport.reply(Reply::json(200, role("r2", Some(Value::Null))));
    let updated = team
        .update_role(
            "r2",
            &UpdateTeamRoleData {
                name: "Developer".into(),
                description: Some(None),
                permissions: vec![RbacPermission::TeamManage, RbacPermission::MembersRead],
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.description, Some(None));
    let request = transport.last();
    assert_eq!(request.method, http::Method::PUT);
    assert_eq!(target(&request), "/team/roles/r2");
    assert_team_auth(&request);
    assert_eq!(
        body_json(&request),
        json!({"name": "Developer", "description": null, "permissions": ["team:manage", "members:read"]})
    );

    transport.reply(Reply::json(201, role("r4", Some(json!("Reads members")))));
    let copy = team
        .duplicate_role(
            "r2",
            &DuplicateTeamRoleData {
                name: "Developer (copy)".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(copy.id, "r4");
    let request = transport.last();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(target(&request), "/team/roles/r2/duplicate");
    assert_team_auth(&request);
    assert_eq!(body_json(&request), json!({"name": "Developer (copy)"}));
}

#[tokio::test]
async fn delete_role_sends_the_replacement_role_only_when_set() {
    let transport = MockTransport::new();
    transport.always(Reply::json(200, json!({"message": "Team role deleted."})));
    let client = team_client(&transport);

    let deleted = client
        .team()
        .delete_role("r2", &Default::default())
        .await
        .unwrap();
    assert_eq!(deleted.message, "Team role deleted.");
    let request = transport.last();
    assert_eq!(request.method, http::Method::DELETE);
    assert_eq!(target(&request), "/team/roles/r2");
    assert_team_auth(&request);
    assert_eq!(request.body, None);

    client
        .team()
        .delete_role(
            "r2",
            &DeleteTeamRoleQuery {
                replacement_role_id: Some("r1".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        target(&transport.last()),
        "/team/roles/r2?replacement_role_id=r1"
    );
}

#[tokio::test]
async fn role_conflicts_and_validation_errors() {
    let transport = MockTransport::new();
    let client = team_client(&transport);

    transport.reply(Reply::json(
        409,
        json!({"message": "The role is still assigned. Choose a replacement role."}),
    ));
    let error = client
        .team()
        .delete_role("r2", &Default::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Conflict(_)), "{error:?}");
    assert_eq!(error.status(), Some(409));
    assert_eq!(
        error.api_error().unwrap().message(),
        "The role is still assigned. Choose a replacement role."
    );

    transport.reply(Reply::json(
        422,
        json!({"message": "The name has already been taken.", "errors": {"name": ["The name has already been taken."]}}),
    ));
    let error = client
        .team()
        .create_role(&CreateTeamRoleData {
            name: "Developer".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    assert_eq!(
        error.api_error().unwrap().errors().unwrap()["name"],
        ["The name has already been taken."]
    );
}

#[tokio::test]
async fn roles_check_their_path_parameters_and_token() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    let team = client.team();
    for id in ["", ".", ".."] {
        let errors = [
            team.retrieve_role(id).await.unwrap_err(),
            team.update_role(id, &Default::default()).await.unwrap_err(),
            team.delete_role(id, &Default::default()).await.unwrap_err(),
            team.duplicate_role(id, &Default::default())
                .await
                .unwrap_err(),
        ];
        for error in errors {
            assert!(matches!(error, Error::Config(_)), "{id:?}: {error:?}");
            assert!(error.to_string().contains("`role_id`"), "{error}");
        }
    }

    let error = sending_client(&transport)
        .team()
        .create_role(&Default::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Config(_)), "{error:?}");
    assert!(error.to_string().contains("team_token"), "{error}");
    assert_eq!(transport.count(), 0);
}

#[tokio::test]
async fn members_list_filters_by_project() {
    let member = json!({"id": "u1", "name": "Jane", "email": "jane@example.com", "role": {"id": "r1", "name": "Developer", "system_key": null}, "project_access": {"scope": "all", "projects": []}, "joined_at": "2026-10-01T00:00:00Z"});
    let transport = MockTransport::new();
    transport.reply(page(json!([member]), None));
    transport.reply(page(json!([]), None));
    let client = team_client(&transport);

    let members = client
        .team()
        .members()
        .list(&ListTeamMembersQuery {
            filter_project_id: Some("p1".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(members.data[0].id, "u1");
    let request = transport.last();
    assert_eq!(request.method, http::Method::GET);
    assert_eq!(target(&request), "/team/members?filter%5Bproject_id%5D=p1");
    assert_team_auth(&request);

    // The iterator keeps the filter on every page; an unknown project is an empty list.
    let mut iterate = client.team().members().iterate(&ListTeamMembersQuery {
        page_size: Some(10),
        filter_project_id: Some("unknown".into()),
        ..Default::default()
    });
    assert!(iterate.next().await.is_none());
    assert_eq!(
        target(&transport.last()),
        "/team/members?page%5Bsize%5D=10&filter%5Bproject_id%5D=unknown"
    );
}

#[tokio::test]
async fn members_remove_deletes_the_member() {
    let transport = MockTransport::new();
    transport.reply(Reply::json(200, json!({"message": "Team member removed."})));
    let client = team_client(&transport);
    let removed = client.team().members().remove("u1").await.unwrap();
    assert_eq!(removed.message, "Team member removed.");
    let request = transport.last();
    assert_eq!(request.method, http::Method::DELETE);
    assert_eq!(target(&request), "/team/members/u1");
    assert_team_auth(&request);
    assert_eq!(request.body, None);
    assert_eq!(header(&request, "content-type"), None);

    transport.reply(Reply::json(
        409,
        json!({"message": "The team owner cannot be removed."}),
    ));
    let error = client.team().members().remove("u0").await.unwrap_err();
    assert!(matches!(error, Error::Conflict(_)), "{error:?}");
    assert_eq!(error.status(), Some(409));
    assert_eq!(
        error.api_error().unwrap().message(),
        "The team owner cannot be removed."
    );

    transport.reply(Reply::json(404, json!({"message": "Not found."})));
    let error = client.team().members().remove("u9").await.unwrap_err();
    assert!(matches!(error, Error::NotFound(_)), "{error:?}");

    let before = transport.count();
    for id in ["", ".", ".."] {
        let error = client.team().members().remove(id).await.unwrap_err();
        assert!(matches!(error, Error::Config(_)), "{id:?}: {error:?}");
        assert!(error.to_string().contains("`user_id`"), "{error}");
    }
    let error = sending_client(&transport)
        .team()
        .members()
        .remove("u1")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("team_token"), "{error}");
    assert_eq!(transport.count(), before);
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

fn project_api_token(id: &str) -> Value {
    json!({"id": id, "project_id": "p1", "name": "Production server", "short_token": false, "created_via": "api", "allowed_ips": [], "last_used_at": null, "created_at": "2026-10-28T09:00:00Z"})
}

#[tokio::test]
async fn project_api_tokens_list_and_iterate() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    let mut legacy = project_api_token("t0");
    legacy["created_via"] = Value::Null;
    legacy["allowed_ips"] = json!(["203.0.113.0/24"]);
    legacy["last_used_at"] = json!("2026-10-28T10:00:00Z");

    transport.reply(page(json!([project_api_token("t1"), legacy]), Some("c2")));
    let page_one = client
        .projects()
        .api_tokens()
        .list(
            "p1",
            &ListProjectApiTokensQuery {
                page_size: Some(2),
                sort: Some(vec![ListProjectApiTokensQuerySortItem::CreatedAtDesc]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page_one.data[0].id, "t1");
    assert_eq!(page_one.data[0].project_id, "p1");
    assert_eq!(page_one.data[0].name, "Production server");
    assert!(!page_one.data[0].short_token);
    assert_eq!(page_one.data[0].created_via, Some(ApiTokenSource::Api));
    assert_eq!(page_one.data[0].last_used_at, None);
    assert_eq!(page_one.data[1].created_via, None);
    assert_eq!(page_one.data[1].allowed_ips, ["203.0.113.0/24"]);
    assert_eq!(
        page_one.data[1].last_used_at.as_deref(),
        Some("2026-10-28T10:00:00Z")
    );
    assert_eq!(page_one.next_cursor.as_deref(), Some("c2"));
    let request = transport.last();
    assert_eq!(request.method, http::Method::GET);
    assert_eq!(
        target(&request),
        "/projects/p1/api-tokens?page%5Bsize%5D=2&sort=-created_at"
    );
    assert_team_auth(&request);
    assert_eq!(request.body, None);

    transport.reply(page(json!([project_api_token("t1")]), Some("c2")));
    transport.reply(page(json!([project_api_token("t2")]), None));
    let mut tokens = client
        .projects()
        .api_tokens()
        .iterate("p 1", &Default::default());
    let mut ids = Vec::new();
    while let Some(token) = tokens.next().await {
        ids.push(token.unwrap().id);
    }
    assert_eq!(ids, ["t1", "t2"]);
    assert_eq!(
        target(&transport.last()),
        "/projects/p%201/api-tokens?page%5Bcursor%5D=c2"
    );
}

#[tokio::test]
async fn project_api_tokens_create_and_delete() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    let mut secret = project_api_token("t1");
    secret["token"] = json!("lm_ProjectTokenValue");

    transport.reply(Reply::json(
        201,
        json!({"data": secret, "smtp": {"host": "smtp.lettermint.co", "ports": [465, 587], "username": "lettermint"}, "message": "API token created."}),
    ));
    let created = client
        .projects()
        .api_tokens()
        .create(
            "p1",
            &StoreProjectApiTokenData {
                name: "Production server".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(created.data.id, "t1");
    assert_eq!(created.data.token, "lm_ProjectTokenValue");
    assert_eq!(created.data.created_via, Some(ApiTokenSource::Api));
    assert_eq!(created.message, "API token created.");
    let smtp = created.smtp.unwrap();
    assert_eq!(smtp.host, "smtp.lettermint.co");
    assert_eq!(smtp.ports, [465, 587]);
    assert_eq!(smtp.username, "lettermint");
    let request = transport.last();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(target(&request), "/projects/p1/api-tokens");
    assert_team_auth(&request);
    assert_eq!(header(&request, "content-type"), Some("application/json"));
    // Without short_token the key is left out.
    assert_eq!(body_json(&request), json!({"name": "Production server"}));

    // Without SMTP for the project, smtp is null.
    secret["short_token"] = json!(true);
    transport.reply(Reply::json(
        201,
        json!({"data": secret, "smtp": null, "message": "API token created."}),
    ));
    let created = client
        .projects()
        .api_tokens()
        .create(
            "p1",
            &StoreProjectApiTokenData {
                name: "Printer".into(),
                short_token: Some(true),
            },
        )
        .await
        .unwrap();
    assert!(created.data.short_token);
    assert_eq!(created.smtp, None);
    assert_eq!(
        body_json(&transport.last()),
        json!({"name": "Printer", "short_token": true})
    );

    transport.reply(Reply::json(200, json!({"message": "API token deleted."})));
    let deleted = client
        .projects()
        .api_tokens()
        .delete("p1", "t/1")
        .await
        .unwrap();
    assert_eq!(deleted.message, "API token deleted.");
    let request = transport.last();
    assert_eq!(request.method, http::Method::DELETE);
    assert_eq!(target(&request), "/projects/p1/api-tokens/t%2F1");
    assert_team_auth(&request);
    assert_eq!(request.body, None);
}

#[tokio::test]
async fn project_api_token_errors_map_to_the_error_variants() {
    let transport = MockTransport::new();
    let client = team_client(&transport);

    transport.reply(Reply::json(
        422,
        json!({"message": "The name field is required.", "errors": {"name": ["The name field is required."]}}),
    ));
    let error = client
        .projects()
        .api_tokens()
        .create("p1", &Default::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    assert_eq!(
        error.api_error().unwrap().errors().unwrap()["name"],
        ["The name field is required."]
    );

    transport.reply(Reply::json(404, json!({"message": "Not found."})));
    let error = client
        .projects()
        .api_tokens()
        .delete("p1", "t9")
        .await
        .unwrap_err();
    assert!(matches!(error, Error::NotFound(_)), "{error:?}");
    assert_eq!(error.status(), Some(404));
}

#[tokio::test]
async fn project_api_tokens_check_their_path_parameters_and_token() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    let tokens = client.projects().api_tokens();
    for id in ["", ".", ".."] {
        let errors = [
            tokens.list(id, &Default::default()).await.unwrap_err(),
            tokens.create(id, &Default::default()).await.unwrap_err(),
            tokens.delete(id, "t1").await.unwrap_err(),
        ];
        for error in errors {
            assert!(matches!(error, Error::Config(_)), "{id:?}: {error:?}");
            assert!(error.to_string().contains("`project_id`"), "{error}");
        }
        let error = tokens.delete("p1", id).await.unwrap_err();
        assert!(matches!(error, Error::Config(_)), "{id:?}: {error:?}");
        assert!(error.to_string().contains("`token_id`"), "{error}");
    }

    let error = sending_client(&transport)
        .projects()
        .api_tokens()
        .create("p1", &Default::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Config(_)), "{error:?}");
    assert!(error.to_string().contains("team_token"), "{error}");
    assert_eq!(transport.count(), 0);
}
