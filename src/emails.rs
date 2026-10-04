//! Sending email: [`Emails`], the [`EmailBuilder`] and message validation.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::time::Duration;

use base64::Engine as _;

use crate::client::Lettermint;
use crate::error::{InputError, Result};
use crate::generated::operations::{self as ops, AuthSurface};
use crate::generated::types::{
    MessageAttachmentInput, MessageTagInput, SandboxResult, SendBatchMailResponse, SendMailRequest,
    SendMailRequestSettings, SendMailResponse,
};
use crate::transport::CallOptions;

const MAX_TAGS: usize = 20;

/// Options of a send: an idempotency key and a timeout for this call.
///
/// ```
/// use lettermint::SendOptions;
///
/// let options = SendOptions::new().idempotency_key("order-1234-confirmation");
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SendOptions {
    idempotency_key: Option<String>,
    timeout: Option<Duration>,
}

impl SendOptions {
    /// No idempotency key and the client's timeout.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sent as the `Idempotency-Key` header. Retrying with the same key does not send the email
    /// again. It applies to this call only and is never stored on the client.
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    /// Overrides the client's timeout for this call.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn into_call(self) -> CallOptions {
        CallOptions {
            auth: None,
            idempotency_key: self.idempotency_key,
            timeout: self.timeout,
        }
    }
}

/// Sends email with the project sending token (`x-lettermint-token`). Returned by
/// [`Lettermint::emails`].
///
/// It holds no message state: every call sends exactly what it is given.
#[derive(Clone)]
pub struct Emails {
    client: Lettermint,
}

impl Emails {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Sends one email. The message is validated first (tags and attachments); an invalid one
    /// returns [`Error::InvalidInput`](crate::Error::InvalidInput) without a request.
    pub async fn send(
        &self,
        message: &SendMailRequest,
        options: SendOptions,
    ) -> Result<SendMailResponse> {
        self.client
            .assert_auth("emails.send", AuthSurface::Sending)?;
        validate_message(message, "")?;
        self.client
            .call::<ops::SendMail>("emails.send", &[], &(), Some(message), options.into_call())
            .await
    }

    /// Sends up to 500 emails in one request. Use [`EmailBuilder::build`] to add a builder.
    pub async fn send_batch(
        &self,
        messages: &[SendMailRequest],
        options: SendOptions,
    ) -> Result<SendBatchMailResponse> {
        self.client
            .assert_auth("emails.send_batch", AuthSurface::Sending)?;
        for (index, message) in messages.iter().enumerate() {
            validate_message(message, &format!("messages[{index}]"))?;
        }
        self.client
            .call_with::<ops::SendBatchMail, _>(
                "emails.send_batch",
                &[],
                messages,
                options.into_call(),
            )
            .await
    }

    /// Starts an email builder. Its setters consume and return the builder; clone a builder to
    /// use it as a template.
    pub fn compose(&self) -> EmailBuilder {
        self.compose_from(SendMailRequest::default())
    }

    /// Starts an email builder from an existing message.
    pub fn compose_from(&self, message: SendMailRequest) -> EmailBuilder {
        EmailBuilder {
            emails: self.clone(),
            message,
        }
    }

    /// Checks the sending token: `GET /ping` returns `pong`.
    pub async fn ping(&self) -> Result<String> {
        let options = CallOptions {
            auth: Some(AuthSurface::Sending),
            ..CallOptions::default()
        };
        let text: String = self
            .client
            .call::<ops::Ping>("emails.ping", &[], &(), None, options)
            .await?;
        Ok(text.trim().to_owned())
    }
}

impl fmt::Debug for Emails {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Emails")
    }
}

/// An email builder, created by [`Emails::compose`].
///
/// The builder owns its message, so nothing is shared between emails. Setters consume the
/// builder and return it; [`send`](Self::send) borrows it, so a builder can be sent again.
/// Clone a builder to reuse it as a template:
///
/// ```no_run
/// # async fn run(lettermint: lettermint::Lettermint) -> lettermint::Result<()> {
/// use lettermint::SendOptions;
///
/// let welcome = lettermint.emails().compose().from("Acme <hello@acme.com>").subject("Welcome");
/// welcome.clone().to("jane@example.com").html("<p>Hi Jane</p>").send(SendOptions::new()).await?;
/// welcome.clone().to("john@example.com").html("<p>Hi John</p>").send(SendOptions::new()).await?;
/// # Ok(())
/// # }
/// ```
///
/// `to`, `cc`, `bcc` and `reply_to` replace their list; `attach` appends. Setters never fail:
/// the message is validated by [`build`](Self::build) and [`send`](Self::send), before any
/// request.
#[derive(Clone)]
pub struct EmailBuilder {
    emails: Emails,
    message: SendMailRequest,
}

impl EmailBuilder {
    /// The sender, for example `Acme <hello@acme.com>`.
    pub fn from(mut self, address: impl Into<String>) -> Self {
        self.message.from = address.into();
        self
    }

    /// Replaces the recipients: one address (`"jane@example.com"`) or a list.
    pub fn to(mut self, addresses: impl IntoAddresses) -> Self {
        self.message.to = addresses.into_addresses();
        self
    }

    /// Replaces the CC recipients.
    pub fn cc(mut self, addresses: impl IntoAddresses) -> Self {
        self.message.cc = Some(addresses.into_addresses());
        self
    }

    /// Replaces the BCC recipients.
    pub fn bcc(mut self, addresses: impl IntoAddresses) -> Self {
        self.message.bcc = Some(addresses.into_addresses());
        self
    }

    /// Replaces the Reply-To addresses.
    pub fn reply_to(mut self, addresses: impl IntoAddresses) -> Self {
        self.message.reply_to = Some(addresses.into_addresses());
        self
    }

    /// The subject line.
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.message.subject = subject.into();
        self
    }

    /// The HTML body.
    pub fn html(mut self, html: impl Into<String>) -> Self {
        self.message.html = Some(Some(html.into()));
        self
    }

    /// Removes the HTML body.
    pub fn clear_html(mut self) -> Self {
        self.message.html = None;
        self
    }

    /// The plain-text body.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.message.text = Some(Some(text.into()));
        self
    }

    /// Removes the plain-text body.
    pub fn clear_text(mut self) -> Self {
        self.message.text = None;
        self
    }

    /// Replaces the custom email headers.
    pub fn headers<K: Into<String>, V: Into<String>>(
        mut self,
        headers: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        self.message.headers = Some(collect_map(headers));
        self
    }

    /// Replaces the metadata (stored with the message, not added as headers).
    pub fn metadata<K: Into<String>, V: Into<String>>(
        mut self,
        metadata: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        self.message.metadata = Some(collect_map(metadata));
        self
    }

    /// The legacy single tag.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.message.tag = Some(Some(tag.into()));
        self
    }

    /// Removes the legacy tag.
    pub fn clear_tag(mut self) -> Self {
        self.message.tag = None;
        self
    }

    /// Replaces the name/value tags: up to 20, or 19 with a legacy [`tag`](Self::tag). Accepts
    /// `(name, value)` pairs or [`MessageTagInput`] values.
    pub fn tags<T: Into<MessageTagInput>>(mut self, tags: impl IntoIterator<Item = T>) -> Self {
        self.message.tags = Some(tags.into_iter().map(Into::into).collect());
        self
    }

    /// The route slug to send through.
    pub fn route(mut self, route: impl Into<String>) -> Self {
        self.message.route = Some(route.into());
        self
    }

    /// Schedules delivery: ISO 8601 (`2026-10-20T09:00:00Z`) or English (`tomorrow 9am`). A time
    /// without a timezone is UTC.
    pub fn scheduled_at(mut self, when: impl Into<String>) -> Self {
        self.message.scheduled_at = Some(when.into());
        self
    }

    /// Removes the scheduled time.
    pub fn clear_scheduled_at(mut self) -> Self {
        self.message.scheduled_at = None;
        self
    }

    /// Per-email settings that override the route settings.
    pub fn settings(mut self, settings: SendMailRequestSettings) -> Self {
        self.message.settings = Some(settings);
        self
    }

    /// The result a Sandbox project simulates for every recipient.
    pub fn sandbox_result(mut self, result: SandboxResult) -> Self {
        self.message.sandbox_result = Some(result);
        self
    }

    /// Adds an attachment.
    pub fn attach(mut self, attachment: Attachment) -> Self {
        self.message
            .attachments
            .get_or_insert_with(Vec::new)
            .push(attachment.into());
        self
    }

    /// The message in the API's wire format, after validation.
    pub fn build(&self) -> Result<SendMailRequest> {
        validate_message(&self.message, "")?;
        Ok(self.message.clone())
    }

    /// The message as composed so far, without validation.
    pub fn message(&self) -> &SendMailRequest {
        &self.message
    }

    /// Validates and sends the email. The builder is unchanged and can be sent again.
    pub async fn send(&self, options: SendOptions) -> Result<SendMailResponse> {
        self.emails.send(&self.message, options).await
    }
}

impl fmt::Debug for EmailBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmailBuilder")
            .field("message", &self.message)
            .finish()
    }
}

fn collect_map<K: Into<String>, V: Into<String>>(
    pairs: impl IntoIterator<Item = (K, V)>,
) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}

/// One address or a list of addresses, for [`EmailBuilder::to`] and the other recipient setters.
pub trait IntoAddresses {
    /// The addresses.
    fn into_addresses(self) -> Vec<String>;
}

impl IntoAddresses for &str {
    fn into_addresses(self) -> Vec<String> {
        vec![self.to_owned()]
    }
}

impl IntoAddresses for String {
    fn into_addresses(self) -> Vec<String> {
        vec![self]
    }
}

impl IntoAddresses for &String {
    fn into_addresses(self) -> Vec<String> {
        vec![self.clone()]
    }
}

impl<T: Into<String>> IntoAddresses for Vec<T> {
    fn into_addresses(self) -> Vec<String> {
        self.into_iter().map(Into::into).collect()
    }
}

impl<T: Into<String>, const N: usize> IntoAddresses for [T; N] {
    fn into_addresses(self) -> Vec<String> {
        self.into_iter().map(Into::into).collect()
    }
}

impl<T: Clone + Into<String>> IntoAddresses for &[T] {
    fn into_addresses(self) -> Vec<String> {
        self.iter().cloned().map(Into::into).collect()
    }
}

impl<T: Clone + Into<String>> IntoAddresses for &Vec<T> {
    fn into_addresses(self) -> Vec<String> {
        self.as_slice().into_addresses()
    }
}

impl<N: Into<String>, V: Into<String>> From<(N, V)> for MessageTagInput {
    fn from((name, value): (N, V)) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// An attachment for [`EmailBuilder::attach`].
///
/// ```
/// use lettermint::Attachment;
///
/// let invoice = Attachment::from_bytes("invoice.pdf", b"%PDF-1.7 ...").content_type("application/pdf");
/// let logo = Attachment::from_base64("logo.png", "iVBORw0KGgo=").content_id("logo");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    filename: String,
    content: String,
    content_type: Option<String>,
    content_id: Option<String>,
}

impl Attachment {
    /// An attachment from raw bytes; the SDK base64-encodes them.
    pub fn from_bytes(filename: impl Into<String>, content: impl AsRef<[u8]>) -> Self {
        Self::from_base64(
            filename,
            base64::engine::general_purpose::STANDARD.encode(content),
        )
    }

    /// An attachment from base64-encoded content.
    pub fn from_base64(filename: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            filename: filename.into(),
            content: content.into(),
            content_type: None,
            content_id: None,
        }
    }

    /// The MIME type, for example `application/pdf`. Detected by the API when omitted.
    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// The Content-ID for inline images referenced as `cid:<content_id>` in the HTML.
    pub fn content_id(mut self, content_id: impl Into<String>) -> Self {
        self.content_id = Some(content_id.into());
        self
    }
}

impl From<Attachment> for MessageAttachmentInput {
    fn from(attachment: Attachment) -> Self {
        Self {
            filename: attachment.filename,
            content: attachment.content,
            content_type: attachment.content_type.map(Some),
            content_id: attachment.content_id.map(Some),
        }
    }
}

/// Checks what the SDK can check before a request: tags and attachments. `prefix` names the
/// message in error fields (`messages[2]`).
pub(crate) fn validate_message(message: &SendMailRequest, prefix: &str) -> Result<(), InputError> {
    let at = |field: &str| {
        if prefix.is_empty() {
            field.to_owned()
        } else {
            format!("{prefix}.{field}")
        }
    };
    let has_legacy_tag = matches!(&message.tag, Some(Some(tag)) if !tag.is_empty());
    if let Some(tags) = &message.tags {
        validate_tags(tags, has_legacy_tag)
            .map_err(|message| InputError::new(at("tags"), message))?;
    }
    if let Some(attachments) = &message.attachments {
        for (index, attachment) in attachments.iter().enumerate() {
            if attachment.filename.is_empty() {
                return Err(InputError::new(
                    format!("{}[{index}]", at("attachments")),
                    "An attachment needs a filename.",
                ));
            }
        }
    }
    Ok(())
}

/// The tag rules of the API: at most 20 tags (19 with a legacy tag), names
/// `^[A-Za-z0-9_-]{1,32}$` not starting with `__lettermint`, values `^[A-Za-z0-9_-]{1,64}$`, and
/// unique, case-sensitive names.
pub(crate) fn validate_tags(tags: &[MessageTagInput], has_legacy_tag: bool) -> Result<(), String> {
    let maximum = if has_legacy_tag {
        MAX_TAGS - 1
    } else {
        MAX_TAGS
    };
    if tags.len() > maximum {
        return Err(if has_legacy_tag {
            format!("A legacy tag and no more than {maximum} message tags are permitted.")
        } else {
            format!("No more than {maximum} message tags are permitted.")
        });
    }
    let mut names = HashSet::new();
    for tag in tags {
        if !token_like(&tag.name, 32) {
            return Err("Message tag names must match ^[A-Za-z0-9_-]{1,32}$.".into());
        }
        if tag.name.to_ascii_lowercase().starts_with("__lettermint") {
            return Err("Message tag names must not start with __lettermint.".into());
        }
        if !token_like(&tag.value, 64) {
            return Err("Message tag values must match ^[A-Za-z0-9_-]{1,64}$.".into());
        }
        if !names.insert(tag.name.as_str()) {
            return Err("Message tag names must be unique (case-sensitive).".into());
        }
    }
    Ok(())
}

fn token_like(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, value: &str) -> MessageTagInput {
        (name, value).into()
    }

    #[test]
    fn tag_rules_match_the_api() {
        assert!(validate_tags(&[tag("campaign", "welcome"), tag("Campaign", "x")], false).is_ok());
        assert!(validate_tags(&[tag(&"a".repeat(32), &"b".repeat(64))], false).is_ok());
        for (tags, needle) in [
            (vec![tag("", "x")], "names must match"),
            (vec![tag(&"a".repeat(33), "x")], "names must match"),
            (vec![tag("not valid!", "x")], "names must match"),
            (vec![tag("ünicode", "x")], "names must match"),
            (
                vec![tag("__lettermint_x", "x")],
                "must not start with __lettermint",
            ),
            (
                vec![tag("__LetterMint", "x")],
                "must not start with __lettermint",
            ),
            (vec![tag("a", "")], "values must match"),
            (vec![tag("a", &"b".repeat(65))], "values must match"),
            (vec![tag("a", "x"), tag("a", "y")], "must be unique"),
        ] {
            let error = validate_tags(&tags, false).unwrap_err();
            assert!(error.contains(needle), "{error}");
        }
        let twenty: Vec<_> = (0..20).map(|i| tag(&format!("t{i}"), "v")).collect();
        assert!(validate_tags(&twenty, false).is_ok());
        assert!(
            validate_tags(&twenty, true)
                .unwrap_err()
                .contains("legacy tag and no more than 19")
        );
        let twenty_one: Vec<_> = (0..21).map(|i| tag(&format!("t{i}"), "v")).collect();
        assert!(
            validate_tags(&twenty_one, false)
                .unwrap_err()
                .contains("No more than 20")
        );
    }

    #[test]
    fn messages_report_the_field() {
        let message = SendMailRequest {
            tags: Some(vec![tag("bad name", "x")]),
            ..Default::default()
        };
        let error = validate_message(&message, "messages[2]").unwrap_err();
        assert_eq!(error.field(), "messages[2].tags");
        let message = SendMailRequest {
            attachments: Some(vec![Attachment::from_base64("", "eA==").into()]),
            ..Default::default()
        };
        assert_eq!(
            validate_message(&message, "").unwrap_err().field(),
            "attachments[0]"
        );
        // An empty legacy tag does not use up a slot.
        let message = SendMailRequest {
            tag: Some(Some(String::new())),
            tags: Some((0..20).map(|i| tag(&format!("t{i}"), "v")).collect()),
            ..Default::default()
        };
        assert!(validate_message(&message, "").is_ok());
    }

    #[test]
    fn attachments_encode_bytes() {
        let input: MessageAttachmentInput = Attachment::from_bytes("a.txt", "attachment A")
            .content_type("text/plain")
            .into();
        assert_eq!(input.content, "YXR0YWNobWVudCBB");
        assert_eq!(input.content_type, Some(Some("text/plain".into())));
        assert_eq!(input.content_id, None);
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"filename": "a.txt", "content": "YXR0YWNobWVudCBB", "content_type": "text/plain"})
        );
    }
}
