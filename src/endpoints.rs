use crate::client::HttpClient;
use crate::error::Result;
use crate::types;
use serde::Serialize;

pub const OPERATION_IDS: &[&str] = &[
    "v1.sendMail",
    "v1.sendBatchMail",
    "v1.ping",
    "domain.index",
    "domain.store",
    "domain.show",
    "domain.destroy",
    "domain.verifyDnsRecords",
    "domain.verifySpecificDnsRecord",
    "domain.updateProjects",
    "v1.ping",
    "v1.blockedFileTypes",
    "message.index",
    "message.show",
    "rescheduleMessage",
    "cancelScheduledMessage",
    "message.events",
    "message.source",
    "message.html",
    "message.text",
    "project.index",
    "project.store",
    "project.show",
    "project.update",
    "project.destroy",
    "project.rotateToken",
    "route.index",
    "route.store",
    "route.show",
    "route.update",
    "route.destroy",
    "route.verifyInboundDomain",
    "stats.index",
    "suppression.index",
    "suppression.store",
    "suppression.destroy",
    "team.show",
    "team.update",
    "team.usage",
    "team.roles",
    "team.members",
    "team.members.show",
    "team.members.assignment.update",
    "webhook.index",
    "webhook.store",
    "webhook.show",
    "webhook.update",
    "webhook.destroy",
    "webhook.test",
    "webhook.regenerateSecret",
    "webhook.deliveries",
    "webhook.showDelivery",
];

type Query<'a> = &'a [(&'a str, &'a str)];

pub struct Domains<'a> {
    client: &'a HttpClient,
}

impl<'a> Domains<'a> {
    pub(crate) fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    pub async fn list(&self, query: Query<'_>) -> Result<types::DomainIndexResponse> {
        self.client.get("/domains", query).await
    }

    pub async fn create<B>(&self, payload: &B) -> Result<types::DomainStoreResponse>
    where
        B: Serialize + Sync,
    {
        self.client.post("/domains", payload).await
    }

    pub async fn retrieve(&self, domain_id: &str) -> Result<types::DomainShowResponse> {
        self.client
            .get(&format!("/domains/{}", segment(domain_id)), &[])
            .await
    }

    pub async fn delete(&self, domain_id: &str) -> Result<types::DomainDestroyResponse> {
        self.client
            .delete(&format!("/domains/{}", segment(domain_id)))
            .await
    }

    pub async fn verify_dns_records(
        &self,
        domain_id: &str,
    ) -> Result<types::DomainVerifyDnsRecordsResponse> {
        self.client
            .post(
                &format!("/domains/{}/dns-records/verify", segment(domain_id)),
                &empty_body(),
            )
            .await
    }

    pub async fn verify_dns_record(
        &self,
        domain_id: &str,
        record_id: &str,
    ) -> Result<types::DomainVerifySpecificDnsRecordResponse> {
        self.client
            .post(
                &format!(
                    "/domains/{}/dns-records/{}/verify",
                    segment(domain_id),
                    segment(record_id)
                ),
                &empty_body(),
            )
            .await
    }

    pub async fn update_projects<B>(
        &self,
        domain_id: &str,
        payload: &B,
    ) -> Result<types::DomainUpdateProjectsResponse>
    where
        B: Serialize + Sync,
    {
        self.client
            .put(
                &format!("/domains/{}/projects", segment(domain_id)),
                payload,
            )
            .await
    }
}

pub struct Messages<'a> {
    client: &'a HttpClient,
}

impl<'a> Messages<'a> {
    pub(crate) fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    pub async fn list(&self, query: Query<'_>) -> Result<types::MessageIndexResponse> {
        self.client.get("/messages", query).await
    }

    pub async fn retrieve(&self, message_id: &str) -> Result<types::MessageShowResponse> {
        self.client
            .get(&format!("/messages/{}", segment(message_id)), &[])
            .await
    }

    pub async fn reschedule(
        &self,
        message_id: &str,
        payload: &types::RescheduleMessageRequest,
    ) -> Result<types::RescheduleMessageResponse> {
        self.client
            .patch(&format!("/messages/{}", segment(message_id)), payload)
            .await
    }

    pub async fn cancel(&self, message_id: &str) -> Result<types::RescheduleMessageResponse> {
        self.client
            .post(
                &format!("/messages/{}/cancel", segment(message_id)),
                &serde_json::json!({}),
            )
            .await
    }

    pub async fn events(
        &self,
        message_id: &str,
        query: Query<'_>,
    ) -> Result<types::MessageEventsResponse> {
        self.client
            .get(&format!("/messages/{}/events", segment(message_id)), query)
            .await
    }

    pub async fn source(&self, message_id: &str) -> Result<String> {
        self.client
            .get_raw(&format!("/messages/{}/source", segment(message_id)), &[])
            .await
    }

    pub async fn html(&self, message_id: &str) -> Result<String> {
        self.client
            .get_raw(&format!("/messages/{}/html", segment(message_id)), &[])
            .await
    }

    pub async fn text(&self, message_id: &str) -> Result<String> {
        self.client
            .get_raw(&format!("/messages/{}/text", segment(message_id)), &[])
            .await
    }
}

pub struct Projects<'a> {
    client: &'a HttpClient,
}

impl<'a> Projects<'a> {
    pub(crate) fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    pub async fn list(&self, query: Query<'_>) -> Result<types::ProjectIndexResponse> {
        self.client.get("/projects", query).await
    }

    pub async fn create<B>(&self, payload: &B) -> Result<types::ProjectStoreResponse>
    where
        B: Serialize + Sync,
    {
        self.client.post("/projects", payload).await
    }

    pub async fn retrieve(&self, project_id: &str) -> Result<types::ProjectShowResponse> {
        self.client
            .get(&format!("/projects/{}", segment(project_id)), &[])
            .await
    }

    pub async fn update<B>(
        &self,
        project_id: &str,
        payload: &B,
    ) -> Result<types::ProjectUpdateResponse>
    where
        B: Serialize + Sync,
    {
        self.client
            .put(&format!("/projects/{}", segment(project_id)), payload)
            .await
    }

    pub async fn delete(&self, project_id: &str) -> Result<types::ProjectDestroyResponse> {
        self.client
            .delete(&format!("/projects/{}", segment(project_id)))
            .await
    }

    pub async fn rotate_token(
        &self,
        project_id: &str,
    ) -> Result<types::ProjectRotateTokenResponse> {
        self.client
            .post(
                &format!("/projects/{}/rotate-token", segment(project_id)),
                &empty_body(),
            )
            .await
    }

    pub async fn routes(
        &self,
        project_id: &str,
        query: Query<'_>,
    ) -> Result<types::RouteIndexResponse> {
        self.client
            .get(&format!("/projects/{}/routes", segment(project_id)), query)
            .await
    }

    pub async fn create_route<B>(
        &self,
        project_id: &str,
        payload: &B,
    ) -> Result<types::RouteStoreResponse>
    where
        B: Serialize + Sync,
    {
        self.client
            .post(
                &format!("/projects/{}/routes", segment(project_id)),
                payload,
            )
            .await
    }
}

pub struct Routes<'a> {
    client: &'a HttpClient,
}

impl<'a> Routes<'a> {
    pub(crate) fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    pub async fn retrieve(&self, route_id: &str) -> Result<types::RouteShowResponse> {
        self.client
            .get(&format!("/routes/{}", segment(route_id)), &[])
            .await
    }

    pub async fn update<B>(&self, route_id: &str, payload: &B) -> Result<types::RouteUpdateResponse>
    where
        B: Serialize + Sync,
    {
        self.client
            .put(&format!("/routes/{}", segment(route_id)), payload)
            .await
    }

    pub async fn delete(&self, route_id: &str) -> Result<types::RouteDestroyResponse> {
        self.client
            .delete(&format!("/routes/{}", segment(route_id)))
            .await
    }

    pub async fn verify_inbound_domain(
        &self,
        route_id: &str,
    ) -> Result<types::RouteVerifyInboundDomainResponse> {
        self.client
            .post(
                &format!("/routes/{}/verify-inbound-domain", segment(route_id)),
                &empty_body(),
            )
            .await
    }
}

pub struct Stats<'a> {
    client: &'a HttpClient,
}

impl<'a> Stats<'a> {
    pub(crate) fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    pub async fn retrieve(&self, query: Query<'_>) -> Result<types::StatsIndexResponse> {
        self.client.get("/stats", query).await
    }
}

pub struct Suppressions<'a> {
    client: &'a HttpClient,
}

impl<'a> Suppressions<'a> {
    pub(crate) fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    pub async fn list(&self, query: Query<'_>) -> Result<types::SuppressionIndexResponse> {
        self.client.get("/suppressions", query).await
    }

    pub async fn create<B>(&self, payload: &B) -> Result<types::SuppressionStoreResponse>
    where
        B: Serialize + Sync,
    {
        self.client.post("/suppressions", payload).await
    }

    pub async fn delete(&self, suppression_id: &str) -> Result<types::SuppressionDestroyResponse> {
        self.client
            .delete(&format!("/suppressions/{}", segment(suppression_id)))
            .await
    }
}

pub struct Team<'a> {
    client: &'a HttpClient,
}

impl<'a> Team<'a> {
    pub(crate) fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    pub async fn retrieve(&self) -> Result<types::TeamShowResponse> {
        self.client.get("/team", &[]).await
    }

    pub async fn update<B>(&self, payload: &B) -> Result<types::TeamUpdateResponse>
    where
        B: Serialize + Sync,
    {
        self.client.put("/team", payload).await
    }

    pub async fn usage(&self) -> Result<types::TeamUsageResponse> {
        self.client.get("/team/usage", &[]).await
    }

    pub async fn roles(&self) -> Result<types::TeamRolesResponse> {
        self.client.get("/team/roles", &[]).await
    }

    pub async fn members(&self, query: Query<'_>) -> Result<types::TeamMembersResponse> {
        self.client.get("/team/members", query).await
    }

    pub async fn member(&self, user_id: &str) -> Result<types::TeamMembersShowResponse> {
        self.client
            .get(&format!("/team/members/{}", segment(user_id)), &[])
            .await
    }

    pub async fn update_member_assignment<B>(
        &self,
        user_id: &str,
        payload: &B,
    ) -> Result<types::TeamMembersAssignmentUpdateResponse>
    where
        B: Serialize + Sync,
    {
        self.client
            .put(
                &format!("/team/members/{}/assignment", segment(user_id)),
                payload,
            )
            .await
    }
}

pub struct Webhooks<'a> {
    client: &'a HttpClient,
}

impl<'a> Webhooks<'a> {
    pub(crate) fn new(client: &'a HttpClient) -> Self {
        Self { client }
    }

    pub async fn list(&self, query: Query<'_>) -> Result<types::WebhookIndexResponse> {
        self.client.get("/webhooks", query).await
    }

    pub async fn create<B>(&self, payload: &B) -> Result<types::WebhookStoreResponse>
    where
        B: Serialize + Sync,
    {
        self.client.post("/webhooks", payload).await
    }

    pub async fn retrieve(&self, webhook_id: &str) -> Result<types::WebhookShowResponse> {
        self.client
            .get(&format!("/webhooks/{}", segment(webhook_id)), &[])
            .await
    }

    pub async fn update<B>(
        &self,
        webhook_id: &str,
        payload: &B,
    ) -> Result<types::WebhookUpdateResponse>
    where
        B: Serialize + Sync,
    {
        self.client
            .put(&format!("/webhooks/{}", segment(webhook_id)), payload)
            .await
    }

    pub async fn delete(&self, webhook_id: &str) -> Result<types::WebhookDestroyResponse> {
        self.client
            .delete(&format!("/webhooks/{}", segment(webhook_id)))
            .await
    }

    pub async fn test(&self, webhook_id: &str) -> Result<types::WebhookTestResponse> {
        self.client
            .post(
                &format!("/webhooks/{}/test", segment(webhook_id)),
                &empty_body(),
            )
            .await
    }

    pub async fn regenerate_secret(
        &self,
        webhook_id: &str,
    ) -> Result<types::WebhookRegenerateSecretResponse> {
        self.client
            .post(
                &format!("/webhooks/{}/regenerate-secret", segment(webhook_id)),
                &empty_body(),
            )
            .await
    }

    pub async fn deliveries(
        &self,
        webhook_id: &str,
        query: Query<'_>,
    ) -> Result<types::WebhookDeliveriesResponse> {
        self.client
            .get(
                &format!("/webhooks/{}/deliveries", segment(webhook_id)),
                query,
            )
            .await
    }

    pub async fn delivery(
        &self,
        webhook_id: &str,
        delivery_id: &str,
    ) -> Result<types::WebhookShowDeliveryResponse> {
        self.client
            .get(
                &format!(
                    "/webhooks/{}/deliveries/{}",
                    segment(webhook_id),
                    segment(delivery_id)
                ),
                &[],
            )
            .await
    }
}

fn segment(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

fn empty_body() -> serde_json::Value {
    serde_json::json!({})
}
