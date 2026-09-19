use async_trait::async_trait;
use lettermint::client::{AuthMode, HttpRequest, HttpResponse, Transport};
use lettermint::{Lettermint, types};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct MockTransport {
    requests: Arc<Mutex<Vec<HttpRequest>>>,
    responses: Arc<Mutex<Vec<HttpResponse>>>,
}

impl MockTransport {
    fn new(responses: Vec<HttpResponse>) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses)),
        }
    }

    fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn send(&self, request: HttpRequest) -> lettermint::Result<HttpResponse> {
        self.requests.lock().unwrap().push(request);
        Ok(self.responses.lock().unwrap().remove(0))
    }
}

fn json_response(body: serde_json::Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        reason: "OK".into(),
        body: body.to_string(),
    }
}

fn text_response(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        reason: "OK".into(),
        body: body.into(),
    }
}

#[tokio::test]
async fn email_entrypoint_uses_sending_auth_and_raw_ping() {
    let transport = MockTransport::new(vec![text_response(" pong\n")]);
    let email = Lettermint::email_with_transport("sending-token", transport.clone()).unwrap();

    assert_eq!(email.ping().await.unwrap(), "pong");

    let request = transport.requests().pop().unwrap();
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, "https://api.lettermint.co/v1/ping");
    assert_eq!(
        request.headers.get("x-lettermint-token").unwrap(),
        "sending-token"
    );
    assert!(!request.headers.contains_key("authorization"));
}

#[tokio::test]
async fn api_entrypoint_uses_bearer_auth_and_raw_ping() {
    let transport = MockTransport::new(vec![text_response(" pong")]);
    let api = Lettermint::api_with_transport("api-token", transport.clone()).unwrap();

    assert_eq!(api.ping().await.unwrap(), "pong");

    let request = transport.requests().pop().unwrap();
    assert_eq!(
        request.headers.get("authorization").unwrap(),
        "Bearer api-token"
    );
    assert!(!request.headers.contains_key("x-lettermint-token"));
}

#[tokio::test]
async fn api_lists_blocked_file_types() {
    let transport = MockTransport::new(vec![json_response(
        serde_json::json!({"extensions": ["exe"], "mime_types": ["application/x-msdownload"]}),
    )]);
    let api = Lettermint::api_with_transport("api-token", transport.clone()).unwrap();

    let response = api.blocked_file_types().await.unwrap();

    assert_eq!(response.extensions, vec!["exe"]);
    let request = transport.requests().pop().unwrap();
    assert_eq!(
        request.url,
        "https://api.lettermint.co/v1/blocked-file-types"
    );
    assert_eq!(request.method, "GET");
}

#[tokio::test]
async fn fluent_email_builder_sends_and_resets_attachment_payload() {
    let transport = MockTransport::new(vec![
        json_response(serde_json::json!({"message_id": "msg_1", "status": "pending"})),
        json_response(serde_json::json!({"message_id": "msg_2", "status": "pending"})),
    ]);
    let email = Lettermint::email_with_transport("sending-token", transport.clone()).unwrap();

    let response = email
        .email()
        .from("sender@example.com")
        .to("first@example.com")
        .subject("First")
        .html("<p>Hello</p>")
        .attach("invoice.pdf", "base64-pdf")
        .idempotency_key("idem-1")
        .send()
        .await
        .unwrap();

    assert_eq!(response.message_id, "msg_1");

    email
        .email()
        .from("sender@example.com")
        .to("second@example.com")
        .subject("Second")
        .send()
        .await
        .unwrap();

    let requests = transport.requests();
    let first_body: serde_json::Value =
        serde_json::from_str(requests[0].body.as_ref().unwrap()).unwrap();
    let second_body: serde_json::Value =
        serde_json::from_str(requests[1].body.as_ref().unwrap()).unwrap();

    assert_eq!(first_body["attachments"][0]["filename"], "invoice.pdf");
    assert!(second_body.get("attachments").is_none());
    assert_eq!(
        requests[0].headers.get("idempotency-key").unwrap(),
        "idem-1"
    );
    assert!(!requests[1].headers.contains_key("idempotency-key"));
}

#[tokio::test]
async fn typed_message_tags_are_validated_and_serialized() {
    use lettermint::types::MessageTag;

    assert!(MessageTag::new("invalid name", "value").is_err());
    assert!(MessageTag::new("__LETTERMINT_internal", "value").is_err());

    let transport = MockTransport::new(vec![json_response(
        serde_json::json!({"message_id": "msg_1", "status": "pending"}),
    )]);
    let email = Lettermint::email_with_transport("sending-token", transport.clone()).unwrap();
    email
        .email()
        .from("sender@example.com")
        .to("recipient@example.com")
        .subject("Tags")
        .text("body")
        .tags([MessageTag::new("campaign", "welcome").unwrap()])
        .send()
        .await
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_str(transport.requests()[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["tags"][0]["name"], "campaign");
}

#[tokio::test]
async fn direct_and_batch_send_support_typed_responses() {
    let transport = MockTransport::new(vec![
        json_response(serde_json::json!({"message_id": "msg_1", "status": "pending"})),
        json_response(serde_json::json!([{"message_id": "msg_2", "status": "queued"}])),
    ]);
    let email = Lettermint::email_with_transport("sending-token", transport.clone()).unwrap();

    let payload = types::SendMailRequest {
        from: "sender@example.com".into(),
        to: vec!["recipient@example.com".into()],
        subject: "Hello".into(),
        ..Default::default()
    };

    assert_eq!(email.send(&payload).await.unwrap().message_id, "msg_1");
    assert_eq!(
        email.send_batch(&[payload]).await.unwrap()[0].message_id,
        "msg_2"
    );
    assert_eq!(
        transport.requests()[1].url,
        "https://api.lettermint.co/v1/send/batch"
    );
}

#[tokio::test]
async fn domain_endpoint_maps_query_path_payload_and_typed_response() {
    let transport = MockTransport::new(vec![
        json_response(serde_json::json!({"data": [{"id": "dom_1", "domain": "example.com"}]})),
        json_response(serde_json::json!({"id": "domain 1", "domain": "example.com"})),
        json_response(serde_json::json!({"id": "dom_2", "domain": "new.example"})),
    ]);
    let api = Lettermint::api_with_transport("api-token", transport.clone()).unwrap();

    let list = api.domains().list(&[("page[size]", "5")]).await.unwrap();
    let show = api.domains().retrieve("domain 1").await.unwrap();
    let created = api
        .domains()
        .create(&serde_json::json!({"domain": "new.example"}))
        .await
        .unwrap();

    assert_eq!(list.data[0].domain, "example.com");
    assert_eq!(show.id, "domain 1");
    assert_eq!(created.domain, "new.example");

    let requests = transport.requests();
    assert_eq!(
        requests[0].url,
        "https://api.lettermint.co/v1/domains?page%5Bsize%5D=5"
    );
    assert_eq!(
        requests[1].url,
        "https://api.lettermint.co/v1/domains/domain%201"
    );
    assert_eq!(requests[2].method, "POST");
}

#[tokio::test]
async fn message_raw_body_endpoints_return_plain_text() {
    let transport = MockTransport::new(vec![
        text_response("raw source"),
        text_response("<p>Hello</p>"),
        text_response("Hello"),
    ]);
    let api = Lettermint::api_with_transport("api-token", transport.clone()).unwrap();

    assert_eq!(
        api.messages().source("message id").await.unwrap(),
        "raw source"
    );
    assert_eq!(
        api.messages().html("message id").await.unwrap(),
        "<p>Hello</p>"
    );
    assert_eq!(api.messages().text("message id").await.unwrap(), "Hello");
    assert_eq!(
        transport.requests()[0].url,
        "https://api.lettermint.co/v1/messages/message%20id/source"
    );
}

#[tokio::test]
async fn process_quarantined_message_uses_typed_api_endpoint() {
    let transport = MockTransport::new(vec![json_response(
        serde_json::json!({"data": {"message_id": "message id", "status": "queued", "webhook_target_count": 1}}),
    )]);
    let api = Lettermint::api_with_transport("api-token", transport.clone()).unwrap();
    assert_eq!(
        api.messages().process("message id").await.unwrap().data["status"],
        "queued"
    );
    assert_eq!(
        transport.requests()[0].url,
        "https://api.lettermint.co/v1/messages/message%20id/process"
    );
}

#[tokio::test]
async fn team_role_and_member_assignment_endpoints_map_requests() {
    let transport = MockTransport::new(vec![
        json_response(serde_json::json!({"data": []})),
        json_response(serde_json::json!({"id": "user/id"})),
        json_response(serde_json::json!({"id": "user/id"})),
    ]);
    let api = Lettermint::api_with_transport("api-token", transport.clone()).unwrap();

    assert!(api.team().roles().await.unwrap().data.is_empty());
    assert_eq!(
        api.team().member("user/id").await.unwrap().id,
        "user/id"
    );
    assert_eq!(
        api.team()
            .update_member_assignment(
                "user/id",
                &types::UpdateTeamMemberAssignmentData {
                    role_id: "role_123".into(),
                    project_access: serde_json::json!({"scope": "all"}),
                },
            )
            .await
            .unwrap()
            .id,
        "user/id"
    );

    let requests = transport.requests();
    assert_eq!(
        requests[0].url,
        "https://api.lettermint.co/v1/team/roles"
    );
    assert_eq!(
        requests[1].url,
        "https://api.lettermint.co/v1/team/members/user%2Fid"
    );
    assert_eq!(requests[2].method, "PUT");
    assert_eq!(
        requests[2].url,
        "https://api.lettermint.co/v1/team/members/user%2Fid/assignment"
    );
    assert!(requests[2]
        .body
        .as_deref()
        .unwrap()
        .contains("\"role_id\":\"role_123\""));
}

#[test]
fn documented_operations_are_exposed() {
    assert_eq!(lettermint::endpoints::OPERATION_IDS.len(), 50);
    assert!(lettermint::endpoints::OPERATION_IDS.contains(&"processInboundMessage"));
    assert!(lettermint::endpoints::OPERATION_IDS.contains(&"v1.sendMail"));
    assert!(lettermint::endpoints::OPERATION_IDS.contains(&"v1.blockedFileTypes"));
    assert!(lettermint::endpoints::OPERATION_IDS.contains(&"team.roles"));
    assert!(lettermint::endpoints::OPERATION_IDS.contains(&"team.members.show"));
    assert!(
        lettermint::endpoints::OPERATION_IDS.contains(&"team.members.assignment.update")
    );
    assert!(lettermint::endpoints::OPERATION_IDS.contains(&"webhook.showDelivery"));
}

#[test]
fn auth_modes_are_distinct() {
    assert_ne!(AuthMode::Sending, AuthMode::Api);
}
