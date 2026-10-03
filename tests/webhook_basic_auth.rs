use lettermint::types::{
    StoreWebhookData, UpdateWebhookData, WebhookBasicAuthData, WebhookData, WebhookListData,
    WebhookSecretData,
};

#[test]
fn credential_debug_output_is_redacted() {
    let credentials = WebhookBasicAuthData {
        username: "fixture-user".into(),
        password: "fixture-password".into(),
    };
    let debug = format!("{:?}", credentials);
    assert!(!debug.contains("fixture-user"));
    assert!(!debug.contains("fixture-password"));
}

#[test]
fn credential_states_round_trip_without_trimming_or_dropping_null() {
    for state in [
        None,
        Some(None),
        Some(Some(WebhookBasicAuthData {
            username: " fixture user ".into(),
            password: "".into(),
        })),
    ] {
        let update = UpdateWebhookData {
            basic_auth: state.clone(),
            ..Default::default()
        };
        let encoded = serde_json::to_value(update).unwrap();
        assert_eq!(encoded.get("basic_auth").is_some(), state.is_some());
        if let Some(None) = state {
            assert!(encoded["basic_auth"].is_null());
        }
        if let Some(Some(_)) = state {
            assert_eq!(encoded["basic_auth"]["password"], "");
            assert_eq!(encoded["basic_auth"]["username"], " fixture user ");
        }
        let decoded: UpdateWebhookData = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.basic_auth, state);
        let create = StoreWebhookData {
            basic_auth: state.clone(),
            ..Default::default()
        };
        let encoded = serde_json::to_value(create).unwrap();
        let decoded: StoreWebhookData = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.basic_auth, state);
    }
}

#[test]
fn all_webhook_read_models_keep_the_safe_credential_flag() {
    let detail: WebhookData = serde_json::from_str(r#"{"has_basic_auth":true}"#).unwrap();
    let list: WebhookListData = serde_json::from_str(r#"{"has_basic_auth":true}"#).unwrap();
    let secret: WebhookSecretData = serde_json::from_str(r#"{"has_basic_auth":true}"#).unwrap();
    assert!(detail.has_basic_auth && list.has_basic_auth && secret.has_basic_auth);
    assert!(
        serde_json::to_value(detail)
            .unwrap()
            .get("basic_auth")
            .is_none()
    );
}
