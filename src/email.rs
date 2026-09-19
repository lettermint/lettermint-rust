use crate::client::EmailClient;
use crate::error::Result;
use crate::types::{
    EmailAttachment, MessageTag, SendBatchMailResponse, SendMailRequest, SendMailResponse,
    TlsPolicy, validate_message_tags,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct EmailSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_opens: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_clicks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsPolicy>,
}

impl EmailClient {
    pub fn email(&self) -> EmailBuilder<'_> {
        EmailBuilder::new(self)
    }

    pub async fn send(&self, payload: &SendMailRequest) -> Result<SendMailResponse> {
        validate_message_tags(
            payload.tags.as_deref().unwrap_or(&[]),
            payload.tag.is_some(),
        )?;
        self.client.post("/send", payload).await
    }

    pub async fn send_batch(&self, payload: &[SendMailRequest]) -> Result<SendBatchMailResponse> {
        for message in payload {
            validate_message_tags(
                message.tags.as_deref().unwrap_or(&[]),
                message.tag.is_some(),
            )?;
        }
        self.client.post("/send/batch", payload).await
    }

    pub async fn send_batch_with_idempotency_key(
        &self,
        payload: &[SendMailRequest],
        key: impl Into<String>,
    ) -> Result<SendBatchMailResponse> {
        for message in payload {
            validate_message_tags(
                message.tags.as_deref().unwrap_or(&[]),
                message.tag.is_some(),
            )?;
        }
        let headers = BTreeMap::from([("idempotency-key".into(), key.into())]);
        self.client
            .post_with_headers("/send/batch", payload, Some(headers))
            .await
    }
}

pub struct EmailBuilder<'a> {
    client: &'a EmailClient,
    payload: SendMailRequest,
    idempotency_key: Option<String>,
}

impl<'a> EmailBuilder<'a> {
    fn new(client: &'a EmailClient) -> Self {
        Self {
            client,
            payload: SendMailRequest::default(),
            idempotency_key: None,
        }
    }

    pub fn from(mut self, email: impl Into<String>) -> Self {
        self.payload.from = email.into();
        self
    }

    pub fn to(mut self, email: impl Into<String>) -> Self {
        self.payload.to.push(email.into());
        self
    }

    pub fn cc(mut self, email: impl Into<String>) -> Self {
        self.payload
            .cc
            .get_or_insert_with(Vec::new)
            .push(email.into());
        self
    }

    pub fn bcc(mut self, email: impl Into<String>) -> Self {
        self.payload
            .bcc
            .get_or_insert_with(Vec::new)
            .push(email.into());
        self
    }

    pub fn reply_to(mut self, email: impl Into<String>) -> Self {
        self.payload
            .reply_to
            .get_or_insert_with(Vec::new)
            .push(email.into());
        self
    }

    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.payload.subject = subject.into();
        self
    }

    pub fn scheduled_at(mut self, scheduled_at: impl Into<String>) -> Self {
        self.payload.scheduled_at = Some(scheduled_at.into());
        self
    }

    pub fn html(mut self, html: impl Into<String>) -> Self {
        self.payload.html = Some(html.into());
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.payload.text = Some(text.into());
        self
    }

    pub fn route(mut self, route: impl Into<String>) -> Self {
        self.payload.route = Some(route.into());
        self
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.payload
            .headers
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value.into());
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.payload
            .metadata
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value.into());
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.payload.tag = Some(tag.into());
        self
    }

    pub fn tags(mut self, tags: impl IntoIterator<Item = MessageTag>) -> Self {
        self.payload.tags = Some(tags.into_iter().collect());
        self
    }

    pub fn attach(self, filename: impl Into<String>, content: impl Into<String>) -> Self {
        self.attach_with_options(filename, content, None, None)
    }

    pub fn attach_with_options(
        mut self,
        filename: impl Into<String>,
        content: impl Into<String>,
        content_id: Option<String>,
        content_type: Option<String>,
    ) -> Self {
        let attachment = EmailAttachment {
            filename: filename.into(),
            content: content.into(),
            content_type,
            content_id,
        };
        self.payload
            .attachments
            .get_or_insert_with(Vec::new)
            .push(serde_json::to_value(attachment).expect("attachment serializes"));
        self
    }

    pub fn settings(mut self, settings: EmailSettings) -> Self {
        self.payload.settings = Some(serde_json::to_value(settings).expect("settings serialize"));
        self
    }

    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub async fn send(mut self) -> Result<SendMailResponse> {
        let mut headers = BTreeMap::new();
        if let Some(key) = self.idempotency_key.take() {
            headers.insert("idempotency-key".into(), key);
        }
        let payload = std::mem::take(&mut self.payload);
        self.client
            .client
            .post_with_headers("/send", &payload, Some(headers))
            .await
    }
}
