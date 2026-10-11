//! Every operation in the generated table is reachable through the public API.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::generated::operations::OPERATIONS;
use crate::types::*;
use crate::{HttpRequest, HttpResponse, Lettermint, Result, SendOptions, Transport};

const BASE_URL: &str = "https://api.example.test/v1";

/// Records `METHOD path` and answers 404, so that every call ends without decoding a body.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<(String, String)>>>);

#[async_trait]
impl Transport for Recorder {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
        let path = request
            .url
            .strip_prefix(BASE_URL)
            .unwrap()
            .split('?')
            .next()
            .unwrap()
            .to_owned();
        self.0
            .lock()
            .unwrap()
            .push((request.method.to_string(), path));
        Ok(HttpResponse::new(404, Default::default(), b"{}".to_vec()))
    }
}

/// The operation key for a recorded request: the one template whose literal segments match.
fn operation_key(method: &str, path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    let matches: Vec<&str> = OPERATIONS
        .iter()
        .filter(|operation| operation.method == method)
        .filter(|operation| {
            let template: Vec<&str> = operation.path.split('/').collect();
            template.len() == segments.len()
                && template
                    .iter()
                    .zip(&segments)
                    .all(|(part, segment)| part.starts_with('{') || part == segment)
        })
        .map(|operation| operation.key)
        .collect();
    assert_eq!(matches.len(), 1, "{method} {path} matches {matches:?}");
    matches[0].to_owned()
}

#[tokio::test]
async fn every_operation_is_reachable() {
    let recorder = Recorder::default();
    let client = Lettermint::builder()
        .sending_token("lm_sending123")
        .team_token("lm_team_team123")
        .base_url(BASE_URL)
        .transport(recorder.clone())
        .build()
        .unwrap();

    let message = SendMailRequest {
        from: "a@example.com".into(),
        to: vec!["b@example.com".into()],
        subject: "s".into(),
        ..Default::default()
    };
    let _ = client.ping().await;
    let _ = client
        .analytics(&AnalyticsQuery {
            metrics: vec![AnalyticsMetric::Delivered],
            ..Default::default()
        })
        .await;
    let _ = client
        .analytics_pages(&AnalyticsQuery {
            metrics: vec![AnalyticsMetric::Delivered],
            ..Default::default()
        })
        .next()
        .await;
    let _ = client.blocked_file_types().await;

    let emails = client.emails();
    let _ = emails.send(&message, SendOptions::new()).await;
    let _ = emails
        .send_batch(std::slice::from_ref(&message), SendOptions::new())
        .await;
    let _ = emails.ping().await;

    let domains = client.domains();
    let _ = domains.list(&Default::default()).await;
    let _ = domains.iterate(&Default::default()).next().await;
    let _ = domains.create(&StoreDomainData::default()).await;
    let _ = domains.retrieve("d", &Default::default()).await;
    let _ = domains.delete("d").await;
    let _ = domains.verify_dns_records("d").await;
    let _ = domains.verify_dns_record("d", "r").await;
    let _ = domains.update_projects("d", &Default::default()).await;

    let messages = client.messages();
    let _ = messages.list(&Default::default()).await;
    let _ = messages.iterate(&Default::default()).next().await;
    let _ = messages.retrieve("m").await;
    let _ = messages.events("m", &Default::default()).await;
    let _ = messages
        .iterate_events("m", &Default::default())
        .next()
        .await;
    let _ = messages.source("m").await;
    let _ = messages.html("m").await;
    let _ = messages.text("m").await;
    let _ = messages.reschedule("m", &Default::default()).await;
    let _ = messages.cancel("m").await;
    let _ = messages.process("m", SendOptions::new()).await;

    let projects = client.projects();
    let _ = projects.list(&Default::default()).await;
    let _ = projects.iterate(&Default::default()).next().await;
    let _ = projects.create(&Default::default()).await;
    let _ = projects.retrieve("p", &Default::default()).await;
    let _ = projects.update("p", &Default::default()).await;
    let _ = projects.delete("p").await;
    #[allow(deprecated)]
    let _ = projects.rotate_token("p").await;
    let forwarding = projects.report_forwarding();
    let _ = forwarding.retrieve("p").await;
    let _ = forwarding.update("p", &Default::default()).await;
    let _ = forwarding.delete("p").await;
    let _ = forwarding.verify("p", &Default::default()).await;
    let _ = forwarding.resend_code("p").await;

    let routes = client.routes();
    let _ = routes.list("p", &Default::default()).await;
    let _ = routes.iterate("p", &Default::default()).next().await;
    let _ = routes
        .create("p", &StoreRouteData::new("Inbound", RouteType::Inbound))
        .await;
    let _ = routes.retrieve("r", &Default::default()).await;
    let _ = routes.update("r", &Default::default()).await;
    let _ = routes.delete("r").await;
    let _ = routes.verify_inbound_domain("r").await;

    let _ = client.stats().retrieve(&Default::default()).await;

    let suppressions = client.suppressions();
    let _ = suppressions.list(&Default::default()).await;
    let _ = suppressions.iterate(&Default::default()).next().await;
    let _ = suppressions
        .create(&StoreSuppressionData::new(
            SuppressionCreateReason::Manual,
            SuppressionCreateScope::Team,
        ))
        .await;
    let _ = suppressions.delete("s").await;

    let team = client.team();
    let _ = team.retrieve(&Default::default()).await;
    let _ = team.update(&Default::default()).await;
    let _ = team.usage().await;
    let _ = team.roles().await;
    let members = team.members();
    let _ = members.list(&Default::default()).await;
    let _ = members.iterate(&Default::default()).next().await;
    let _ = members.retrieve("u").await;
    let access = UpdateTeamMemberAssignmentDataProjectAccess::new(ProjectAccessScope::All);
    let _ = members
        .update_assignment("u", &UpdateTeamMemberAssignmentData::new("member", access))
        .await;

    let webhooks = client.webhooks();
    let _ = webhooks.list(&Default::default()).await;
    let _ = webhooks.iterate(&Default::default()).next().await;
    let _ = webhooks.create(&Default::default()).await;
    let _ = webhooks.retrieve("w").await;
    let _ = webhooks.update("w", &Default::default()).await;
    let _ = webhooks.delete("w").await;
    let _ = webhooks.test("w").await;
    let _ = webhooks.regenerate_secret("w").await;
    let deliveries = webhooks.deliveries();
    let _ = deliveries.list("w", &Default::default()).await;
    let _ = deliveries.iterate("w", &Default::default()).next().await;
    let _ = deliveries.retrieve("w", "x").await;

    let reached: BTreeSet<String> = recorder
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|(method, path)| operation_key(method, path))
        .collect();
    let table: BTreeSet<String> = OPERATIONS
        .iter()
        .map(|operation| operation.key.to_owned())
        .collect();
    assert_eq!(table.len(), 58);
    let missing: Vec<_> = table.difference(&reached).collect();
    assert!(missing.is_empty(), "not reachable: {missing:?}");
    assert_eq!(reached, table);
}

#[test]
fn operation_keys_match_methods_and_paths() {
    for operation in OPERATIONS {
        assert_eq!(
            operation.key,
            format!("{} {}", operation.method, operation.path)
        );
        for name in operation.path_params {
            assert!(operation.path.contains(&format!("{{{name}}}")));
        }
    }
}
