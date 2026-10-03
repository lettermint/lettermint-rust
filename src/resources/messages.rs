use crate::client::Lettermint;
use crate::emails::SendOptions;
use crate::error::Result;
use crate::generated::operations as ops;
use crate::generated::types::{
    ListMessageEventsQuery, ListMessageEventsResponse, ListMessagesQuery, ListMessagesResponse,
    MessageData, MessageEventData, MessageListData, ProcessInboundMessageResponse,
    RescheduleMessageRequest, ScheduledMessage,
};
use crate::pagination::Paginator;
use crate::transport::CallOptions;

/// Sent and received messages. Returned by [`Lettermint::messages`].
///
/// Team token; [`reschedule`](Self::reschedule) and [`cancel`](Self::cancel) also accept the
/// sending token when no team token is configured, so a sending-only client can cancel the
/// scheduled email it sent.
#[derive(Clone)]
pub struct Messages {
    client: Lettermint,
}

super::opaque_debug!(Messages);

impl Messages {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Lists messages, one page at a time.
    pub async fn list(&self, query: &ListMessagesQuery) -> Result<ListMessagesResponse> {
        self.client
            .call::<ops::ListMessages>("messages.list", &[], query, None, CallOptions::default())
            .await
    }

    /// Iterates over every message, following `next_cursor`.
    pub fn iterate(&self, query: &ListMessagesQuery) -> Paginator<MessageListData> {
        self.client
            .paginate::<ops::ListMessages>("messages.iterate", &[], query)
    }

    /// A message.
    pub async fn retrieve(&self, message_id: &str) -> Result<MessageData> {
        self.client
            .call::<ops::GetMessage>(
                "messages.retrieve",
                &[message_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Lists the events of a message, one page at a time.
    pub async fn events(
        &self,
        message_id: &str,
        query: &ListMessageEventsQuery,
    ) -> Result<ListMessageEventsResponse> {
        self.client
            .call::<ops::ListMessageEvents>(
                "messages.events",
                &[message_id],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Iterates over every event of a message, following `next_cursor`.
    pub fn iterate_events(
        &self,
        message_id: &str,
        query: &ListMessageEventsQuery,
    ) -> Paginator<MessageEventData> {
        self.client.paginate::<ops::ListMessageEvents>(
            "messages.iterate_events",
            &[message_id],
            query,
        )
    }

    /// The raw RFC 822 source.
    pub async fn source(&self, message_id: &str) -> Result<String> {
        self.client
            .call::<ops::GetMessageSource>(
                "messages.source",
                &[message_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// The HTML body.
    pub async fn html(&self, message_id: &str) -> Result<String> {
        self.client
            .call::<ops::GetMessageHtml>(
                "messages.html",
                &[message_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// The plain-text body.
    pub async fn text(&self, message_id: &str) -> Result<String> {
        self.client
            .call::<ops::GetMessageText>(
                "messages.text",
                &[message_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Moves a scheduled message to another delivery time. Team token if configured, otherwise
    /// the sending token.
    pub async fn reschedule(
        &self,
        message_id: &str,
        body: &RescheduleMessageRequest,
    ) -> Result<ScheduledMessage> {
        self.client
            .call::<ops::RescheduleMessage>(
                "messages.reschedule",
                &[message_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Cancels a scheduled message. Team token if configured, otherwise the sending token.
    pub async fn cancel(&self, message_id: &str) -> Result<ScheduledMessage> {
        self.client
            .call::<ops::CancelScheduledMessage>(
                "messages.cancel",
                &[message_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Releases one quarantined inbound message for webhook delivery. Takes an optional
    /// idempotency key.
    pub async fn process(
        &self,
        message_id: &str,
        options: SendOptions,
    ) -> Result<ProcessInboundMessageResponse> {
        self.client
            .call::<ops::ProcessInboundMessage>(
                "messages.process",
                &[message_id],
                &(),
                None,
                options.into_call(),
            )
            .await
    }
}
