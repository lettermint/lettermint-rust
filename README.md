# Lettermint Rust SDK

[![Crates.io Version](https://img.shields.io/crates/v/lettermint?style=flat-square)](https://crates.io/crates/lettermint)
[![Crates.io Downloads](https://img.shields.io/crates/d/lettermint?style=flat-square)](https://crates.io/crates/lettermint)
[![docs.rs](https://img.shields.io/docsrs/lettermint?style=flat-square)](https://docs.rs/lettermint)
[![GitHub Tests](https://img.shields.io/github/actions/workflow/status/lettermint/lettermint-rust/ci.yml?branch=main&label=tests&style=flat-square)](https://github.com/lettermint/lettermint-rust/actions?query=workflow%3ACI+branch%3Amain)
[![License](https://img.shields.io/github/license/lettermint/lettermint-rust?style=flat-square)](https://github.com/lettermint/lettermint-rust/blob/main/LICENSE-MIT)
[![Join our Discord server](https://img.shields.io/discord/1305510095588819035?logo=discord&logoColor=eee&label=Discord&labelColor=464ce5&color=0D0E28&cacheSeconds=43200)](https://lettermint.co/r/discord)

The official Rust SDK for [Lettermint](https://lettermint.co): send email and manage your team through the Lettermint API.

Upgrading from 1.x? Read [UPGRADE.md](UPGRADE.md).

## Installation

```toml
[dependencies]
lettermint = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The SDK needs Rust 1.98 or later and a Tokio runtime (it uses [`reqwest`](https://crates.io/crates/reqwest) with rustls).

## Quick start

Create a client with a project sending token and send an email:

```rust,no_run
use lettermint::{Lettermint, SendOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lettermint = Lettermint::builder()
        .sending_token(std::env::var("LETTERMINT_PROJECT_TOKEN")?)
        .build()?;

    let response = lettermint
        .emails()
        .compose()
        .from("Acme <hello@acme.com>")
        .to("jane@example.com")
        .subject("Welcome to Acme")
        .html("<p>Thanks for signing up.</p>")
        .text("Thanks for signing up.")
        .send(SendOptions::new())
        .await?;

    println!("{} {}", response.message_id, response.status); // "…", "pending"
    Ok(())
}
```

## Tokens

Lettermint has two kinds of API tokens:

| Builder method | Token | Used by | Sent as |
| --- | --- | --- | --- |
| `sending_token` | Project sending token (`lm_…`) | `lettermint.emails()` | `x-lettermint-token` header |
| `team_token` | Team API token (`lm_team_…`) | Every other part (domains, messages, projects, …) | `Authorization: Bearer` header |

Pass one or both:

```rust,no_run
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let lettermint = lettermint::Lettermint::builder()
    .sending_token(std::env::var("LETTERMINT_PROJECT_TOKEN")?)
    .team_token(std::env::var("LETTERMINT_TEAM_TOKEN")?)
    .build()?;
# Ok(())
# }
```

Each part uses its own token and never falls back to the other one. If the token a method needs is missing, it returns `Error::Config` naming the option (``domains.list needs `team_token` ``) before any request. `lettermint.ping()` uses the team token when it is set, otherwise the sending token. `messages().reschedule()` and `messages().cancel()` accept either token in the same way.

You can also pass a single token and let the SDK choose its kind by the format: `lm_team_` followed by letters and digits is a team token, and `lm_` followed by letters and digits is a sending token.

```rust,no_run
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let lettermint = lettermint::Lettermint::new(std::env::var("LETTERMINT_TEAM_TOKEN")?)?;
// With options:
let lettermint = lettermint::Lettermint::builder()
    .token(std::env::var("LETTERMINT_PROJECT_TOKEN")?)
    .timeout(std::time::Duration::from_secs(10))
    .build()?;
# Ok(())
# }
```

Any other format, such as an SSO verification token (`lm_sso_…`), an OAuth token or an empty string, is an `Error::Config`; pass it with `sending_token()` or `team_token()` instead. Error messages never contain the token.

### Options

| Builder method | Default | Description |
| --- | --- | --- |
| `sending_token`, `team_token`, `token` | | The tokens (see above). |
| `base_url` | `https://api.lettermint.co/v1` | API base URL. |
| `timeout` | 30 seconds | Request timeout. It covers the whole request, including reading the body. |
| `transport` | `ReqwestTransport` | The HTTP transport (see [Custom HTTP clients](#custom-http-clients)). |

The client holds no per-request state. It is cheap to clone (clones share one connection pool), `Send + Sync`, and safe to share across tasks: create it once. `lettermint.with_timeout(duration)?` returns a copy with another timeout.

## Sending email

### The email builder

`emails().compose()` returns an `EmailBuilder` that owns its message, so nothing is shared between emails. Setters take the builder by value and return it; `send()` borrows it, so a builder can be sent again. Clone a builder to use it as a template:

```rust,no_run
# use lettermint::{Lettermint, SendOptions};
# async fn run(lettermint: Lettermint) -> lettermint::Result<()> {
let welcome = lettermint
    .emails()
    .compose()
    .from("Acme <hello@acme.com>")
    .subject("Welcome to Acme")
    .tags([("campaign", "welcome")]);

welcome.clone().to("jane@example.com").html("<p>Hi Jane</p>").send(SendOptions::new()).await?;
welcome.clone().to("john@example.com").html("<p>Hi John</p>").send(SendOptions::new()).await?;
# Ok(())
# }
```

To build an email over several statements, rebind the builder:

```rust,no_run
# use lettermint::{Lettermint, SendOptions};
# async fn run(lettermint: Lettermint, accountant: Option<&str>) -> lettermint::Result<()> {
let mut email = lettermint.emails().compose().from("billing@acme.com").to("jane@example.com").subject("Your invoice");
if let Some(accountant) = accountant {
    email = email.cc(accountant);
}
email.html("<p>Invoice attached.</p>").send(SendOptions::new()).await?;
# Ok(())
# }
```

| Method | Description |
| --- | --- |
| `from(address)` | Sender, for example `Acme <hello@acme.com>`. |
| `to(..)`, `cc(..)`, `bcc(..)`, `reply_to(..)` | Replace the list. Take one address or a list (`["a@x", "b@x"]`, a `Vec`, a slice). |
| `subject(text)` | Subject line. |
| `html(html)`, `text(text)`, `clear_html()`, `clear_text()` | Bodies. |
| `headers([(name, value)])` | Replaces the custom email headers. |
| `metadata([(key, value)])` | Replaces the metadata (stored with the message, not added as headers). |
| `tags([(name, value)])`, `tag(name)`, `clear_tag()` | Name/value tags, and the legacy single tag. |
| `route(slug)` | The route to send through. |
| `scheduled_at(when)`, `clear_scheduled_at()` | Delivery time: ISO 8601 or English such as `tomorrow 9am`. |
| `settings(SendMailRequestSettings)` | Per-email settings that override the route. |
| `sandbox_result(SandboxResult)` | The result a Sandbox project simulates. |
| `attach(Attachment)` | Adds an attachment. |
| `send(SendOptions)` | Validates and sends the email. |
| `build()` | Validates and returns the message in API format (`SendMailRequest`). |

`emails().compose_from(message)` starts a builder from an existing `SendMailRequest`. Setters never fail: the message is validated by `build()` and `send()`, before any request.

### Plain structs

`emails().send()` takes the message in the API's format, `types::SendMailRequest`. Optional nullable fields (`html`, `text`, `tag`) are `Option<Option<String>>`: `None` leaves the field out and `Some(None)` sends `null`.

```rust,no_run
# use lettermint::{Lettermint, SendOptions, types::SendMailRequest};
# async fn run(lettermint: Lettermint) -> lettermint::Result<()> {
let message = SendMailRequest {
    from: "Acme <hello@acme.com>".into(),
    to: vec!["jane@example.com".into()],
    reply_to: Some(vec!["support@acme.com".into()]),
    subject: "Your order has shipped".into(),
    html: Some(Some("<p>On its way.</p>".into())),
    metadata: Some([("order_id".to_owned(), "1234".to_owned())].into()),
    ..Default::default()
};
lettermint.emails().send(&message, SendOptions::new()).await?;
# Ok(())
# }
```

### Batch sending

Send up to 500 emails in one request. Add a builder with `build()`:

```rust,no_run
# use lettermint::{Lettermint, SendOptions, types::SendMailRequest};
# async fn run(lettermint: Lettermint, first: SendMailRequest) -> lettermint::Result<()> {
let emails = lettermint.emails();
let second = emails.compose().from("hello@acme.com").to("john@example.com").subject("Hi John").text("Hello").build()?;
let results = emails.send_batch(&[first, second], SendOptions::new()).await?;
# Ok(())
# }
```

### Idempotency

Pass an idempotency key to make retries safe. The API processes a key once, so a retry with the same key does not send the email again. The key applies only to the call it is passed to.

```rust,no_run
# use lettermint::{EmailBuilder, Lettermint, SendOptions, types::SendMailRequest};
# async fn run(lettermint: Lettermint, message: SendMailRequest, builder: EmailBuilder, order_id: u64) -> lettermint::Result<()> {
lettermint.emails().send(&message, SendOptions::new().idempotency_key(format!("order-{order_id}-confirmation"))).await?;
builder.send(SendOptions::new().idempotency_key("welcome-jane")).await?;
# Ok(())
# }
```

The SDK never retries on its own. `SendOptions::timeout(duration)` overrides the client's timeout for one send.

### Scheduling

```rust,no_run
# use lettermint::{Lettermint, SendOptions, types::{RescheduleMessageRequest, MessageStatus}};
# async fn run(lettermint: Lettermint) -> lettermint::Result<()> {
let result = lettermint
    .emails()
    .compose()
    .from("hello@acme.com")
    .to("jane@example.com")
    .subject("Your trial ends tomorrow")
    .text("…")
    .scheduled_at("2026-10-20T09:00:00Z")
    .send(SendOptions::new())
    .await?;

if result.status == MessageStatus::Scheduled {
    println!("{:?}", result.scheduled_at);
}

let messages = lettermint.messages();
messages
    .reschedule(&result.message_id, &RescheduleMessageRequest { scheduled_at: "2026-10-21T09:00:00Z".into() })
    .await?;
messages.cancel(&result.message_id).await?;
# Ok(())
# }
```

### Tags

`tags()` accepts up to 20 case-sensitive name/value tags (19 when the legacy `tag()` is also set). Names match `^[A-Za-z0-9_-]{1,32}$`, may not start with `__lettermint` and must be unique. Values match `^[A-Za-z0-9_-]{1,64}$`. The SDK checks this before the request and returns `Error::InvalidInput` with the field (`tags`, or `messages[2].tags` in a batch).

### Attachments

```rust,no_run
# use lettermint::{Attachment, Lettermint, SendOptions};
# async fn run(lettermint: Lettermint, logo_base64: String) -> Result<(), Box<dyn std::error::Error>> {
let pdf = std::fs::read("invoice.pdf")?;
lettermint
    .emails()
    .compose()
    .from("billing@acme.com")
    .to("jane@example.com")
    .subject("Your invoice")
    .html(r#"<img src="cid:logo"> Your invoice is attached."#)
    .attach(Attachment::from_bytes("invoice.pdf", pdf).content_type("application/pdf"))
    .attach(Attachment::from_base64("logo.png", logo_base64).content_id("logo"))
    .send(SendOptions::new())
    .await?;
# Ok(())
# }
```

`Attachment::from_bytes` base64-encodes the content. `lettermint.blocked_file_types()` lists the extensions and MIME types the API rejects.

## Team API

With a team token, the client manages domains, messages, projects, routes, statistics, suppressions, the team and webhooks:

```rust,no_run
use lettermint::Lettermint;
use lettermint::types::{GetStatsQuery, StoreDomainData, StoreProjectData};

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let lettermint = Lettermint::builder().team_token(std::env::var("LETTERMINT_TEAM_TOKEN")?).build()?;

let domain = lettermint.domains().create(&StoreDomainData { domain: "acme.com".into() }).await?;
lettermint.domains().verify_dns_records(&domain.id).await?;

let project = lettermint.projects().create(&StoreProjectData { name: "Production".into(), ..Default::default() }).await?;

let stats = lettermint
    .stats()
    .retrieve(&GetStatsQuery { from: "2026-10-01".into(), to: "2026-10-31".into(), ..Default::default() })
    .await?;
let html = lettermint.messages().html("message-id").await?;
# Ok(())
# }
```

| Method | Methods of the sub-client |
| --- | --- |
| `domains()` | `list`, `iterate`, `create`, `retrieve`, `delete`, `verify_dns_records`, `verify_dns_record`, `update_projects` |
| `messages()` | `list`, `iterate`, `retrieve`, `events`, `iterate_events`, `source`, `html`, `text`, `reschedule`, `cancel`, `process` |
| `projects()` | `list`, `iterate`, `create`, `retrieve`, `update`, `delete`, `rotate_token` (deprecated by the API) |
| `projects().report_forwarding()` | `retrieve`, `update`, `delete`, `verify`, `resend_code` |
| `routes()` | `list(project_id, ..)`, `iterate(project_id, ..)`, `create(project_id, ..)`, `retrieve`, `update`, `delete`, `verify_inbound_domain` |
| `stats()` | `retrieve` |
| `suppressions()` | `list`, `iterate`, `create`, `delete` |
| `team()` | `retrieve`, `update`, `usage`, `roles` |
| `team().members()` | `list`, `iterate`, `retrieve`, `update_assignment` |
| `webhooks()` | `list`, `iterate`, `create`, `retrieve`, `update`, `delete`, `test`, `regenerate_secret` |
| `webhooks().deliveries()` | `list(webhook_id, ..)`, `iterate(webhook_id, ..)`, `retrieve(webhook_id, delivery_id)` |
| (the client) | `ping`, `analytics`, `analytics_pages`, `blocked_file_types` |

Request types that have required enum fields have a `new` constructor with the required fields, for example `StoreRouteData::new("Inbound", RouteType::Inbound)`.

### Query parameters and pagination

Query parameters are typed structs with one field per parameter. The SDK sends them in the API's bracket syntax (`page[size]=30&filter[status]=verified&sort=-created_at`): lists of values are joined with commas, lists of objects are indexed and booleans are sent as `1`/`0`.

```rust,no_run
# use lettermint::Lettermint;
use lettermint::types::{DomainStatus, ListDomainsQuery, ListDomainsQuerySortItem};

# async fn run(lettermint: Lettermint) -> lettermint::Result<()> {
let page = lettermint
    .domains()
    .list(&ListDomainsQuery {
        page_size: Some(30),
        filter_status: Some(DomainStatus::Verified),
        sort: Some(vec![ListDomainsQuerySortItem::CreatedAtDesc]),
        ..Default::default()
    })
    .await?;
println!("{} {:?}", page.data.len(), page.next_cursor);
# Ok(())
# }
```

Every list has an `iterate` method that returns a `Paginator`: it follows `next_cursor` until the last page and stops when the API repeats a cursor. Read it with its `next()` method, or as a `futures::Stream`:

```rust,no_run
# use lettermint::Lettermint;
use lettermint::types::{ListMessagesQuery, MessageStatus};

# async fn run(lettermint: Lettermint) -> lettermint::Result<()> {
let query = ListMessagesQuery { filter_status: Some(MessageStatus::HardBounced), ..Default::default() };
let mut messages = lettermint.messages().iterate(&query);
while let Some(message) = messages.next().await {
    let message = message?;
    println!("{} {:?}", message.id, message.subject);
}
# Ok(())
# }
```

The paginator requests the next page only when you get to it; drop it to stop early.

IDs in paths are URL-encoded. An empty ID, `.` or `..` is an `Error::Config`, returned before the request.

### Analytics

`lettermint.analytics(&query)` runs one analytics query. `metrics` is the only required field; by default the API returns a summary of the last 30 days:

```rust,no_run
# use lettermint::Lettermint;
use lettermint::types::{AnalyticsMetric, AnalyticsQuery};

# async fn run(lettermint: Lettermint) -> lettermint::Result<()> {
let result = lettermint
    .analytics(&AnalyticsQuery {
        metrics: vec![AnalyticsMetric::Delivered, AnalyticsMetric::Bounced, AnalyticsMetric::DeliveryRate],
        from: Some("2026-10-01".into()),
        to: Some("2026-10-31".into()),
        timezone: Some("Europe/Amsterdam".into()),
        ..Default::default()
    })
    .await?;

if let Some(summary) = &result.data.summary {
    println!("{:?}", summary.metrics.delivery_rate); // Some(Some(0.9836)), or Some(None) when there is no data
}
println!("{} {}", result.meta.partial, result.meta.effective_to);
# Ok(())
# }
```

Add `include` to ask for a `time_series` or a `breakdown`. A breakdown needs `group_by`, and the API returns its rows in pages of `limit` (at most 200). `analytics_pages()` follows `pagination.next_cursor` for you. It returns a `Paginator` that yields one whole response per request, so each page keeps its `meta` and `pagination`:

```rust,no_run
# use lettermint::Lettermint;
use lettermint::types::{
    AnalyticsBreakdownRow, AnalyticsGroupDimension, AnalyticsMetric, AnalyticsQuery, AnalyticsSection, AnalyticsSort,
    AnalyticsSortDirection,
};

# async fn run(lettermint: Lettermint) -> lettermint::Result<()> {
let query = AnalyticsQuery {
    metrics: vec![AnalyticsMetric::Delivered, AnalyticsMetric::Bounced],
    include: Some(vec![AnalyticsSection::Breakdown]),
    group_by: Some(vec![AnalyticsGroupDimension::RecipientDomain]),
    sort: Some(AnalyticsSort::new(AnalyticsMetric::Bounced, AnalyticsSortDirection::Desc)),
    limit: Some(200),
    ..Default::default()
};

let mut rows: Vec<AnalyticsBreakdownRow> = Vec::new();
let mut pages = lettermint.analytics_pages(&query);
while let Some(page) = pages.next().await {
    let page = page?;
    if page.pagination.truncated {
        eprintln!("More groups exist than the API ranks.");
    }
    rows.extend(page.data.breakdown.unwrap_or_default());
}
# Ok(())
# }
```

A cursor expires 60 seconds after its response, so read the next page promptly. An expired cursor is an `Error::Validation` with `errors()["cursor"]`; run the query again to start over.

A few things to know when you read a response:

- A metric is `Some(None)` (the API's `null`) when the API cannot measure it for that row or bucket, and a rate is `Some(None)` when its denominator is zero. `Some(Some(0))` means a measured zero. A metric the query did not select is `None`.
- `data.summary`, `data.time_series` and `data.breakdown` are `Some` only when `include` asks for them. `previous`, `change` and `meta.comparison` are `Some` only with `compare`.
- `smtp_response_group` can be used in `group_by` but not as a filter dimension.
- Analytics can answer `503` or `504` when a query takes too long or the service is busy. Both are an `Error::Server`; see [Errors](#errors).

### Cancellation and timeouts

Futures are cancelled by dropping them, for example with `tokio::time::timeout` or `tokio::select!`. Each request also has the client's timeout (30 seconds by default), which covers the response headers and the body; use `lettermint.with_timeout(duration)?` for a client with another one.

## Errors

Every method returns `lettermint::Result<T>`, with `lettermint::Error`:

| Variant | When | Details |
| --- | --- | --- |
| `Authentication(ApiError)` | HTTP 401 | |
| `Permission(ApiError)` | HTTP 403 | |
| `NotFound(ApiError)` | HTTP 404 | |
| `Conflict(ApiError)` | HTTP 409 | |
| `Validation(ApiError)` | HTTP 422 | `errors()` holds the field errors |
| `RateLimit(ApiError)` | HTTP 429 | `retry_after()` holds the `Retry-After` delay |
| `Server(ApiError)` | HTTP 5xx | `retry_after()` holds the `Retry-After` delay, when the API sent one |
| `Api(ApiError)` | Any other 4xx | |
| `Timeout { timeout }` | No complete response within the timeout | |
| `Connection { source }` | The request failed (DNS, TLS, refused, reset) | |
| `UnexpectedResponse(..)` | An empty or non-JSON body where JSON was expected, a body that does not match the documented type, or an error page such as a proxy's HTML 502 | `status()`, `body_excerpt()` |
| `Redirect { status }` | A 3xx response. Redirects are never followed, so tokens never go elsewhere. | |
| `Config(ConfigError)` | A missing or unrecognised token, an invalid option or ID | |
| `InvalidInput(InputError)` | The SDK rejected the request before sending it, such as invalid tags | `field()` |
| `WebhookVerification(..)` | A webhook delivery is not genuine | `reason()` |

`ApiError` has `status()`, `code()`, `message()`, `details()` and `body()`; `code` and `message` come from the API's error body (`{ "error": { "code", "message", "details" } }` or `{ "message", "errors" }`). `Error::status()` and `Error::code()` work on every variant, and `Error::api_error()` returns the `ApiError` of any HTTP variant. The enum is `#[non_exhaustive]`: keep a wildcard arm.

```rust,no_run
# use lettermint::{EmailBuilder, Error, SendOptions};
# async fn run(builder: EmailBuilder, key: String) -> lettermint::Result<()> {
match builder.send(SendOptions::new().idempotency_key(key.clone())).await {
    Ok(response) => println!("{}", response.message_id),
    Err(Error::Validation(error)) => eprintln!("{} {:?}", error.message(), error.errors()),
    Err(Error::RateLimit(error)) => {
        tokio::time::sleep(error.retry_after().unwrap_or(std::time::Duration::from_secs(1))).await;
        // then retry with the same idempotency key
    }
    Err(Error::Timeout { .. }) => {
        // The outcome is unknown. Retry with the same idempotency key.
    }
    Err(error) => return Err(error),
}
# Ok(())
# }
```

Errors never contain request headers or tokens: the SDK removes the configured tokens from error bodies and messages. The `Debug` output of the client, its builder, the sub-clients, `EmailBuilder`, `HttpRequest` and `Webhook` shows tokens and secrets as `[redacted]`.

## Webhooks

Verify each webhook delivery before you trust it. Use the webhook's signing secret (`whsec_…`), not an API token, and pass the **raw** request body: the signature covers the exact bytes, so parsing and re-serializing the JSON breaks it.

```rust,no_run
use lettermint::Webhook;

# fn handle(raw_body: &[u8], headers: &http::HeaderMap) -> Result<(), Box<dyn std::error::Error>> {
let webhook = Webhook::new(std::env::var("LETTERMINT_WEBHOOK_SECRET")?)?;

let payload = webhook.verify(raw_body, headers)?;
println!("{} {}", payload.event, payload.data);
# Ok(())
# }
```

`verify(raw_body, headers)` takes the body as bytes or a string, and the headers as an `http::HeaderMap` (axum, hyper, reqwest), a `HashMap` or `BTreeMap` of strings, or a slice or `Vec` of `(name, value)` pairs. It requires `X-Lettermint-Signature` and `X-Lettermint-Delivery` (header names are case-insensitive), checks the HMAC-SHA256 signature in constant time (any `v1` signature may match), checks that the delivery header equals the signed timestamp and that the timestamp is within the tolerance, and returns the payload: `event` (an open `WebhookEvent`), `data`, `id`, `timestamp` and the other fields in `extra`. Otherwise it returns a `WebhookVerificationError` with a `reason()`: `signature_header_missing`, `signature_header_malformed`, `delivery_header_missing`, `delivery_timestamp_mismatch`, `timestamp_out_of_tolerance`, `signature_mismatch`, `body_invalid` or `payload_invalid`.

### axum

```rust,ignore
use axum::{body::Bytes, http::{HeaderMap, StatusCode}};
use lettermint::Webhook;

async fn lettermint_webhook(headers: HeaderMap, body: Bytes) -> StatusCode {
    let webhook = Webhook::new(std::env::var("LETTERMINT_WEBHOOK_SECRET").unwrap()).unwrap();
    match webhook.verify(&body, &headers) {
        Ok(payload) => {
            // Handle payload.event and payload.data here.
            StatusCode::NO_CONTENT
        }
        Err(_) => StatusCode::BAD_REQUEST,
    }
}
```

### Options and lower-level verification

The default tolerance is 300 seconds in either direction. Change it with `Webhook::new(secret)?.with_tolerance(Duration::from_secs(60))`. Zero accepts only the current second; it does not disable the check. A valid signature does not prevent a repeated delivery within the tolerance, so track `payload.id` if you must not process an event twice.

If the headers are not at hand, call `webhook.verify_signature(raw_body, signature_header, Some(delivery_header))`. `verify_at` and `verify_signature_at` take the current time as an argument, for tests and replays. Decode `data` into your own type with `payload.data_as::<T>()`.

## Types

Request and response types are generated from the Lettermint API specification and live in `lettermint::types`, for example `SendMailRequest`, `SendMailResponse`, `DomainData` and `ListDomainsResponse` (a `CursorPage<DomainListData>`).

- **Open enums.** Enums are `#[non_exhaustive]` and have an `Other(String)` variant, so a value that the API adds later decodes and serializes back unchanged. Match with a wildcard arm, or compare with `as_str()`.
- **Optional and nullable.** An optional field is `Option<T>`; a required field that may be `null` is `Option<T>` that the API always sends; an optional field that may be `null` is `Option<Option<T>>`. For `UpdateWebhookData::basic_auth`, `None` keeps the credentials, `Some(None)` removes them and `Some(Some(..))` replaces them.
- **Required fields are required.** A response without a documented required field is an `Error::UnexpectedResponse`, not a value with an empty default. Unknown fields are ignored.
- **New fields** may be added in minor releases. Build request structs with `..Default::default()` or their `new` constructor.

## Custom HTTP clients

The default transport is `ReqwestTransport`, which never follows redirects. To configure reqwest (a proxy, custom TLS roots), pass your `reqwest::ClientBuilder` to `ReqwestTransport::from_builder`; the SDK still disables redirects on it:

```rust,no_run
# fn main() -> lettermint::Result<()> {
let transport = lettermint::ReqwestTransport::from_builder(reqwest::Client::builder().https_only(true))?;
let lettermint = lettermint::Lettermint::builder().token("lm_...").transport(transport).build()?;
# Ok(())
# }
```

To use another HTTP client, or to test without a network, implement the `Transport` trait. A transport must send the request as given, read the whole body, never follow redirects and never retry. The SDK enforces the timeout around it.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps --locked
```

`src/generated/` is generated by the private [SDK generator](https://github.com/lettermint/sdk-generator). Do not edit it by hand. With a checkout of the generator, `scripts/generate.sh` regenerates the files and `scripts/generate.sh --check` verifies them; set `LETTERMINT_SDK_GENERATOR` to the checkout (default `../sdk-generator`). Without the generator, as in CI, `--check` only verifies the generated headers.

## License

Licensed under either the [MIT license](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option.
