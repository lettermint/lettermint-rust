use lettermint::types::RouteData;

#[test]
fn optional_nullable_inbound_route_domain_round_trips() {
    for domain in [Some("incoming.example.com"), None] {
        let mut payload = serde_json::json!({"id":"route_1","project_id":"project_1","slug":"incoming","name":"Incoming","route_type":"inbound","is_default":false,"created_at":"2026-10-01T12:00:00Z","updated_at":"2026-10-01T12:00:00Z"});
        payload["inbound_route_domain"] = serde_json::json!(domain);
        let route: RouteData = serde_json::from_value(payload).unwrap();
        assert_eq!(route.inbound_route_domain.as_deref(), domain);
        let encoded = serde_json::to_value(route).unwrap();
        let decoded: RouteData = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.inbound_route_domain.as_deref(), domain);
    }
    let absent: RouteData = serde_json::from_str(r#"{"id":"route_1","project_id":"project_1","slug":"incoming","name":"Incoming","route_type":"inbound","is_default":false,"created_at":"2026-10-01T12:00:00Z","updated_at":"2026-10-01T12:00:00Z"}"#).unwrap();
    assert!(absent.inbound_route_domain.is_none());
}
