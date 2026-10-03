mod common;

use std::time::Duration;

use common::*;
use lettermint::{Error, Lettermint, SendOptions, TokenKind};

#[test]
fn a_token_string_is_classified_by_prefix() {
    let team = Lettermint::builder()
        .token("lm_team_abc123")
        .transport(MockTransport::new())
        .build()
        .unwrap();
    assert!(team.has_team_token() && !team.has_sending_token());
    let sending = Lettermint::builder()
        .token("lm_abc123")
        .transport(MockTransport::new())
        .build()
        .unwrap();
    assert!(sending.has_sending_token() && !sending.has_team_token());
    assert_eq!(TokenKind::detect("lm_team_x").unwrap(), TokenKind::Team);
}

#[test]
fn unrecognised_tokens_are_config_errors_without_the_token() {
    for token in [
        "",
        "lm_",
        "lm_team_",
        "lm_sso_SsoToken123",
        "sk_live_abc123",
        "eyJhbGciOiJIUzI1NiJ9.e30.c2ln",
    ] {
        let error = Lettermint::new(token).unwrap_err();
        assert!(matches!(error, Error::Config(_)), "{token}: {error:?}");
        assert!(
            error.to_string().starts_with("Unrecognised token format"),
            "{error}"
        );
        if token.len() > 3 {
            assert!(!error.to_string().contains(token));
            assert!(!format!("{error:?}").contains(token));
        }
    }
}

#[test]
fn a_client_needs_a_token() {
    let error = Lettermint::builder()
        .transport(MockTransport::new())
        .build()
        .unwrap_err();
    assert!(matches!(error, Error::Config(_)));
    assert!(
        error
            .to_string()
            .contains("sending token, a team token or both")
    );
}

#[test]
fn explicit_tokens_are_checked() {
    for (builder, option) in [
        (Lettermint::builder().sending_token(""), "sending_token"),
        (Lettermint::builder().team_token("has space"), "team_token"),
    ] {
        let error = builder.transport(MockTransport::new()).build().unwrap_err();
        assert!(matches!(error, Error::Config(_)));
        assert!(error.to_string().contains(option), "{error}");
    }
    // An SSO token can still be passed explicitly.
    assert!(
        Lettermint::builder()
            .team_token("lm_sso_token")
            .transport(MockTransport::new())
            .build()
            .is_ok()
    );
}

#[test]
fn token_and_explicit_token_of_the_same_kind_conflict() {
    let error = Lettermint::builder()
        .token("lm_abc")
        .sending_token("lm_def")
        .transport(MockTransport::new())
        .build()
        .unwrap_err();
    assert!(matches!(error, Error::Config(_)));
    let client = Lettermint::builder()
        .token("lm_abc")
        .team_token("lm_team_def")
        .transport(MockTransport::new())
        .build()
        .unwrap();
    assert!(client.has_sending_token() && client.has_team_token());
}

#[test]
fn options_are_checked() {
    let build = |builder: lettermint::LettermintBuilder| {
        builder
            .sending_token(SENDING_TOKEN)
            .transport(MockTransport::new())
            .build()
    };
    assert!(matches!(
        build(Lettermint::builder().timeout(Duration::ZERO)),
        Err(Error::Config(_))
    ));
    assert!(matches!(
        build(Lettermint::builder().base_url("not a url")),
        Err(Error::Config(_))
    ));
    let client = build(Lettermint::builder().base_url("http://localhost:8080/v1/")).unwrap();
    assert_eq!(client.base_url(), "http://localhost:8080/v1");
    assert_eq!(client.timeout(), Duration::from_secs(30));
    assert_eq!(
        client
            .with_timeout(Duration::from_secs(5))
            .unwrap()
            .timeout(),
        Duration::from_secs(5)
    );
    assert!(client.with_timeout(Duration::ZERO).is_err());
}

#[test]
fn the_default_transport_builds() {
    let client = Lettermint::new("lm_abc123").unwrap();
    assert_eq!(client.base_url(), "https://api.lettermint.co/v1");
}

#[tokio::test]
async fn each_part_uses_its_own_token() {
    let transport = MockTransport::new();
    let client = both_client(&transport);

    client
        .emails()
        .compose()
        .from("a@example.com")
        .to("b@example.com")
        .subject("Hi")
        .text("Hi")
        .send(SendOptions::new())
        .await
        .unwrap();
    let request = transport.last();
    assert_eq!(header(&request, "x-lettermint-token"), Some(SENDING_TOKEN));
    assert_eq!(header(&request, "authorization"), None);

    transport.reply(Reply::json(200, serde_json::json!({"data": [], "path": null, "per_page": 30, "next_cursor": null, "next_page_url": null, "prev_cursor": null, "prev_page_url": null})));
    client.domains().list(&Default::default()).await.unwrap();
    let request = transport.last();
    assert_eq!(
        header(&request, "authorization"),
        Some(format!("Bearer {TEAM_TOKEN}").as_str())
    );
    assert_eq!(header(&request, "x-lettermint-token"), None);
}

#[tokio::test]
async fn ping_and_scheduled_message_calls_use_either_token() {
    let transport = MockTransport::new();
    transport.always(Reply::text(200, "text/html; charset=UTF-8", "pong\n"));
    assert_eq!(sending_client(&transport).ping().await.unwrap(), "pong");
    assert_eq!(
        header(&transport.last(), "x-lettermint-token"),
        Some(SENDING_TOKEN)
    );
    assert_eq!(team_client(&transport).ping().await.unwrap(), "pong");
    assert!(header(&transport.last(), "authorization").is_some());
    // Both tokens: the team token wins; emails().ping() always uses the sending token.
    let both = both_client(&transport);
    both.ping().await.unwrap();
    assert!(header(&transport.last(), "authorization").is_some());
    both.emails().ping().await.unwrap();
    assert_eq!(
        header(&transport.last(), "x-lettermint-token"),
        Some(SENDING_TOKEN)
    );
    assert_eq!(header(&transport.last(), "authorization"), None);

    let scheduled =
        serde_json::json!({"message_id": "m1", "status": "canceled", "scheduled_at": null});
    transport.always(Reply::json(200, scheduled));
    sending_client(&transport)
        .messages()
        .cancel("m1")
        .await
        .unwrap();
    assert_eq!(
        header(&transport.last(), "x-lettermint-token"),
        Some(SENDING_TOKEN)
    );
}

#[tokio::test]
async fn a_missing_token_is_a_config_error_that_names_the_option() {
    let transport = MockTransport::new();
    let error = sending_client(&transport)
        .domains()
        .list(&Default::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Config(_)));
    assert!(
        error
            .to_string()
            .contains("domains.list needs `team_token`"),
        "{error}"
    );
    let error = team_client(&transport)
        .emails()
        .send(&Default::default(), SendOptions::new())
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("emails.send needs `sending_token`"),
        "{error}"
    );
    let error = team_client(&transport).emails().ping().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("emails.ping needs `sending_token`")
    );
    assert_eq!(transport.count(), 0);
}

#[test]
fn debug_output_never_shows_tokens() {
    let transport = MockTransport::new();
    let client = both_client(&transport);
    let builder = Lettermint::builder()
        .sending_token(SENDING_TOKEN)
        .team_token(TEAM_TOKEN)
        .token(SENDING_TOKEN);
    let outputs = [
        format!("{client:?}"),
        format!("{client:#?}"),
        format!("{builder:?}"),
        format!("{:?}", client.emails()),
        format!("{:?}", client.emails().compose().from("a@example.com")),
        format!("{:?}", client.domains()),
        format!("{:?}", client.projects().report_forwarding()),
        format!("{:?}", client.team().members()),
        format!("{:?}", client.webhooks().deliveries()),
        format!("{:?}", client.messages().iterate(&Default::default())),
    ];
    for output in outputs {
        assert!(!output.contains(SENDING_TOKEN), "{output}");
        assert!(!output.contains(TEAM_TOKEN), "{output}");
    }
    assert!(format!("{client:?}").contains("[redacted]"));
}

#[test]
fn clients_are_send_sync_and_static() {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<Lettermint>();
    assert_send_sync::<lettermint::EmailBuilder>();
    assert_send_sync::<lettermint::Emails>();
    assert_send_sync::<lettermint::resources::Domains>();
    assert_send_sync::<lettermint::Webhook>();
    assert_send_sync::<lettermint::Error>();
    fn assert_send<T: Send>() {}
    assert_send::<lettermint::Paginator<lettermint::types::DomainListData>>();
}
