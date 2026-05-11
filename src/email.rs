use crate::client::EmailClient;
use crate::error::Result;
use crate::types::{EmailAttachment, SendBatchMailResponse, SendMailRequest, SendMailResponse};
use std::collections::BTreeMap;

impl EmailClient {
    pub fn email(&self) -> EmailBuilder<'_> {
        EmailBuilder::new(self)
    }

    pub async fn send(&self, payload: &SendMailRequest) -> Result<SendMailResponse> {
        self.client.post("/send", payload).await
    }

    pub async fn send_batch(&self, payload: &[SendMailRequest]) -> Result<SendBatchMailResponse> {
        self.client.post("/send/batch", payload).await
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

    pub fn attach(mut self, filename: impl Into<String>, content: impl Into<String>) -> Self {
        let attachment = EmailAttachment {
            filename: filename.into(),
            content: content.into(),
            content_type: None,
            content_id: None,
        };
        self.payload
            .attachments
            .get_or_insert_with(Vec::new)
            .push(serde_json::to_value(attachment).expect("attachment serializes"));
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
