use crate::client::Lettermint;
use crate::error::Result;
use crate::generated::operations as ops;
use crate::generated::types::{
    ListWebhookDeliveriesQuery, ListWebhookDeliveriesResponse, ListWebhooksQuery,
    ListWebhooksResponse, MessageResponse, StoreWebhookData, TestWebhookResponse,
    UpdateWebhookData, WebhookData, WebhookDeliveryData, WebhookDeliveryListData, WebhookListData,
    WebhookMutationResponse, WebhookSecretResponse,
};
use crate::pagination::Paginator;
use crate::transport::CallOptions;

/// Webhook endpoints. Team token. Returned by [`Lettermint::webhooks`]. To verify incoming
/// deliveries, use [`Webhook`](crate::Webhook).
#[derive(Clone)]
pub struct Webhooks {
    client: Lettermint,
}

/// Delivery attempts of a webhook. Team token. Returned by [`Webhooks::deliveries`].
#[derive(Clone)]
pub struct WebhookDeliveries {
    client: Lettermint,
}

super::opaque_debug!(Webhooks, WebhookDeliveries);

impl Webhooks {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Delivery attempts of a webhook.
    pub fn deliveries(&self) -> WebhookDeliveries {
        WebhookDeliveries {
            client: self.client.clone(),
        }
    }

    /// Lists webhooks, one page at a time.
    pub async fn list(&self, query: &ListWebhooksQuery) -> Result<ListWebhooksResponse> {
        self.client
            .call::<ops::ListWebhooks>("webhooks.list", &[], query, None, CallOptions::default())
            .await
    }

    /// Iterates over every webhook, following `next_cursor`.
    pub fn iterate(&self, query: &ListWebhooksQuery) -> Paginator<WebhookListData> {
        self.client
            .paginate::<ops::ListWebhooks>("webhooks.iterate", &[], query)
    }

    /// Creates a webhook. The response holds its signing secret once.
    pub async fn create(&self, body: &StoreWebhookData) -> Result<WebhookSecretResponse> {
        self.client
            .call::<ops::CreateWebhook>(
                "webhooks.create",
                &[],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// A webhook.
    pub async fn retrieve(&self, webhook_id: &str) -> Result<WebhookData> {
        self.client
            .call::<ops::GetWebhook>(
                "webhooks.retrieve",
                &[webhook_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Updates a webhook. `basic_auth: None` keeps the credentials, `Some(None)` removes them.
    pub async fn update(
        &self,
        webhook_id: &str,
        body: &UpdateWebhookData,
    ) -> Result<WebhookMutationResponse> {
        self.client
            .call::<ops::UpdateWebhook>(
                "webhooks.update",
                &[webhook_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Deletes a webhook.
    pub async fn delete(&self, webhook_id: &str) -> Result<MessageResponse> {
        self.client
            .call::<ops::DeleteWebhook>(
                "webhooks.delete",
                &[webhook_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Sends a `webhook.test` delivery.
    pub async fn test(&self, webhook_id: &str) -> Result<TestWebhookResponse> {
        self.client
            .call::<ops::TestWebhook>(
                "webhooks.test",
                &[webhook_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Replaces the signing secret. The response holds the new secret once.
    pub async fn regenerate_secret(&self, webhook_id: &str) -> Result<WebhookSecretResponse> {
        self.client
            .call::<ops::RegenerateWebhookSecret>(
                "webhooks.regenerate_secret",
                &[webhook_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }
}

impl WebhookDeliveries {
    /// Lists the deliveries of a webhook, one page at a time.
    pub async fn list(
        &self,
        webhook_id: &str,
        query: &ListWebhookDeliveriesQuery,
    ) -> Result<ListWebhookDeliveriesResponse> {
        self.client
            .call::<ops::ListWebhookDeliveries>(
                "webhooks.deliveries.list",
                &[webhook_id],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Iterates over every delivery of a webhook, following `next_cursor`.
    pub fn iterate(
        &self,
        webhook_id: &str,
        query: &ListWebhookDeliveriesQuery,
    ) -> Paginator<WebhookDeliveryListData> {
        self.client.paginate::<ops::ListWebhookDeliveries>(
            "webhooks.deliveries.iterate",
            &[webhook_id],
            query,
        )
    }

    /// One delivery attempt.
    pub async fn retrieve(
        &self,
        webhook_id: &str,
        delivery_id: &str,
    ) -> Result<WebhookDeliveryData> {
        self.client
            .call::<ops::GetWebhookDelivery>(
                "webhooks.deliveries.retrieve",
                &[webhook_id, delivery_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }
}
