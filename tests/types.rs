use std::collections::HashSet;
use std::str::FromStr;

use lettermint::types::{
    AnalyticsDimension, CursorPage, DeleteSuppressionResponse, DeleteSuppressionResponseStatus,
    DomainListData, MessageStatus, RouteData, ScheduledMessage, UpdateWebhookData,
    WebhookBasicAuthData, WebhookEvent,
};
use serde_json::json;

fn route(extra: serde_json::Value) -> serde_json::Value {
    let mut value = json!({"id": "r1", "project_id": "p1", "slug": "in", "name": "In", "route_type": "inbound", "is_default": false, "created_at": "2026-10-01T00:00:00Z", "updated_at": "2026-10-01T00:00:00Z"});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    value
}

#[test]
fn open_enums_keep_unknown_values() {
    let status: MessageStatus = serde_json::from_value(json!("delivered")).unwrap();
    assert_eq!(status, MessageStatus::Delivered);
    let future: MessageStatus = serde_json::from_value(json!("teleported")).unwrap();
    assert_eq!(future, MessageStatus::Other("teleported".into()));
    assert_eq!(serde_json::to_value(&future).unwrap(), json!("teleported"));
    assert_eq!(future.to_string(), "teleported");
    // Known values are normalised; equality and hashing use the wire value.
    assert_eq!(
        MessageStatus::Other("delivered".into()),
        MessageStatus::Delivered
    );
    let set: HashSet<MessageStatus> = [
        MessageStatus::Delivered,
        MessageStatus::Other("delivered".into()),
    ]
    .into();
    assert_eq!(set.len(), 1);
    assert_eq!(
        MessageStatus::from_str("hard_bounced").unwrap(),
        MessageStatus::HardBounced
    );
    assert_eq!(MessageStatus::from("pending"), "pending");
    assert!(MessageStatus::KNOWN.contains(&"scheduled"));
    assert!(serde_json::from_value::<MessageStatus>(json!(1)).is_err());
}

#[test]
fn enum_values_with_symbols_get_readable_variants() {
    assert_eq!(WebhookEvent::MessageDelivered.as_str(), "message.delivered");
    assert_eq!(
        lettermint::types::ListDomainsQuerySortItem::CreatedAtDesc.as_str(),
        "-created_at"
    );
    assert_eq!(
        lettermint::types::RbacPermission::ProjectTokensRead.as_str(),
        "project_tokens:read"
    );
    // A catalogue dimension or a tag dimension.
    let tag = AnalyticsDimension::from("tag:campaign");
    assert_eq!(serde_json::to_value(&tag).unwrap(), json!("tag:campaign"));
    assert_eq!(
        AnalyticsDimension::from("subject"),
        AnalyticsDimension::Subject
    );
}

#[test]
fn required_fields_are_required() {
    let page = json!({"data": [], "path": null, "per_page": 30, "next_cursor": null, "next_page_url": null, "prev_cursor": null, "prev_page_url": null});
    assert!(serde_json::from_value::<CursorPage<DomainListData>>(page.clone()).is_ok());
    // A required nullable field must be present.
    let mut missing = page.clone();
    missing.as_object_mut().unwrap().remove("next_cursor");
    assert!(serde_json::from_value::<CursorPage<DomainListData>>(missing).is_err());
    // A required field may not be null.
    let mut null = page;
    null["per_page"] = json!(null);
    assert!(serde_json::from_value::<CursorPage<DomainListData>>(null).is_err());
    assert!(
        serde_json::from_value::<ScheduledMessage>(json!({"message_id": "m1", "status": null}))
            .is_err()
    );
    let scheduled: ScheduledMessage =
        serde_json::from_value(json!({"message_id": "m1", "status": null, "scheduled_at": null}))
            .unwrap();
    assert_eq!(scheduled.status, None);
}

#[test]
fn optional_nullable_fields_have_three_states() {
    let absent: RouteData = serde_json::from_value(route(json!({}))).unwrap();
    assert_eq!(absent.inbound_route_domain, None);
    let null: RouteData =
        serde_json::from_value(route(json!({"inbound_route_domain": null}))).unwrap();
    assert_eq!(null.inbound_route_domain, Some(None));
    let value: RouteData =
        serde_json::from_value(route(json!({"inbound_route_domain": "in.example.com"}))).unwrap();
    assert_eq!(
        value.inbound_route_domain,
        Some(Some("in.example.com".into()))
    );
    for decoded in [absent, null, value] {
        let encoded = serde_json::to_value(&decoded).unwrap();
        assert_eq!(
            serde_json::from_value::<RouteData>(encoded).unwrap(),
            decoded
        );
    }
}

#[test]
fn update_requests_distinguish_keep_remove_and_set() {
    let keep = UpdateWebhookData::default();
    assert_eq!(serde_json::to_value(&keep).unwrap(), json!({}));
    let remove = UpdateWebhookData {
        basic_auth: Some(None),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&remove).unwrap(),
        json!({"basic_auth": null})
    );
    let set = UpdateWebhookData {
        basic_auth: Some(Some(WebhookBasicAuthData {
            username: "user".into(),
            password: "pass".into(),
        })),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&set).unwrap(),
        json!({"basic_auth": {"username": "user", "password": "pass"}})
    );
}

#[test]
fn merged_responses_decode_every_variant() {
    let removed: DeleteSuppressionResponse =
        serde_json::from_value(json!({"success": true, "status": "removed", "message": "Removed"}))
            .unwrap();
    assert_eq!(removed.status, DeleteSuppressionResponseStatus::Removed);
    assert_eq!(removed.ticket_identifier, None);
    let review: DeleteSuppressionResponse = serde_json::from_value(
        json!({"success": true, "status": "review_pending", "message": "In review", "ticket_identifier": "T-1"}),
    )
    .unwrap();
    assert_eq!(review.ticket_identifier.as_deref(), Some("T-1"));
    assert_eq!(review.status.as_str(), "review_pending");
}
