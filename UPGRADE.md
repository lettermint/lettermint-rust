# Upgrade guide

- [Upgrade from 1.x to 2.0](#upgrade-from-1x-to-20)
- [Upgrade from 0.3 to 1.0](#upgrade-from-03-to-10)

# Upgrade from 1.x to 2.0

1.x no longer receives updates, including fixes. Upgrade to 2.0 to keep getting them.

2.0 is a new major version. Cargo does not select it for `lettermint = "1"`; change the requirement to `lettermint = "2"` when the application is ready. The main reasons are correctness and safety:

- 1.x decoded responses into closed enums. When the API added a status, a successful send failed to decode, and callers that retried sent the email twice. In 2.0 every enum is open.
- 1.x followed redirects with the `x-lettermint-token` header, had no request timeout, and printed tokens and webhook secrets in `Debug` output.
- 1.x filled missing required response fields with defaults, so broken responses looked valid.

## Highlights

- One client: `Lettermint::builder().sending_token(..).team_token(..).build()?`, or `Lettermint::new(token)?`. It replaces `Lettermint::email()`, `Lettermint::api()`, `EmailClient` and `ApiClient`.
- Each part uses its own token: `emails()` uses the sending token, the Team API the team token. The SDK never falls back to the other token.
- The idempotency key is a per-call option (`SendOptions`) on every send, including `emails().send()`.
- Tags are validated before every request, on every path.
- Typed errors for every outcome: one `Error` enum with HTTP variants (`NotFound`, `Validation`, `RateLimit`, …), timeouts, connection failures, redirects and unexpected responses, with `status()` and `code()`.
- Redirects are never followed, so tokens never reach another host. Requests time out after 30 seconds by default; the timeout covers the body.
- Tokens and secrets never appear in `Debug` output or in errors.
- Open enums (`Other(String)`), required fields that are required, and optional and nullable fields kept apart.
- Typed query structs instead of `&[("page[size]", "10")]`, and `iterate()` paginators that follow `next_cursor`.
- Types are generated from the current API specification and use its names (see [Type names](#type-names)).
- Webhook verification requires the delivery header, accepts any matching `v1` signature and returns a typed payload.
- The minimum Rust version stays 1.98 (edition 2024).

## Upgrade with a coding agent

You can let a coding agent (Claude Code, Codex, Cursor, Copilot, …) do the upgrade. Copy this instruction into the agent from your project's root, then review its changes:

````text
Upgrade this project from the `lettermint` Rust SDK 1.x to 2.0.

1. Set `lettermint = "2"` in every Cargo.toml that depends on it and run `cargo update -p lettermint`. 2.0 needs Rust 1.98 or newer and a Tokio runtime: check `rust-version`, rust-toolchain files, CI workflows and Dockerfiles, and report anything older.
2. Read the upgrade guide before changing code: `UPGRADE.md` in the installed crate (`find ~/.cargo/registry/src -path '*lettermint-2.*/UPGRADE.md'`), or https://github.com/lettermint/lettermint-rust/blob/main/UPGRADE.md. Treat it as the source of truth and don't guess APIs; when unsure, read the crate's source in the same folder or run `cargo doc -p lettermint --open`.
3. Find every use of the SDK: `use lettermint`, `lettermint::`, `Lettermint::email(`, `Lettermint::api(`, `email_with_transport`, `api_with_transport`, `EmailClient`, `ApiClient`, `.email()`, `.idempotency_key(`, `send_batch_with_idempotency_key`, `.attach(`, `attach_with_options`, `EmailSettings`, `MessageTag::new`, `Webhook::new`, `verify_headers`, `Transport`, `HttpRequest`, `HttpResponse`, `OPERATION_IDS`, the 1.x `Error` variants, and the 1.x type names from the guide's type-name table.
4. Rewrite each use following the guide's before/after examples:
   - Create one client with `Lettermint::builder().sending_token(..)`, adding `.team_token(..)` only where the Team API is used, and `.build()?`. Keep the project's existing environment variable names.
   - Replace `client.email()` builder chains with `lettermint.emails().compose()`; recipient setters now replace the list and take one address or a list. Pass the idempotency key as `SendOptions::new().idempotency_key(..)` to `send()` / `send_batch()`; use `SendOptions::new()` when there is none.
   - Attachments become `Attachment::from_base64(..)` or `Attachment::from_bytes(..)` with `.content_type(..)` / `.content_id(..)`. Tags become `(name, value)` pairs.
   - Team API: use the same client, typed query structs (`ListDomainsQuery { page_size: Some(10), ..Default::default() }`) instead of `&[("page[size]", "10")]`, and the moved methods from the guide's table.
   - Errors: match the 2.0 `Error` variants (`Error::Validation`, `Error::RateLimit`, …) and keep a wildcard arm.
   - Webhooks: `Webhook::new(secret)?` and `webhook.verify(raw_body, &headers)`. Keep passing the raw request body, keep the secret's `whsec_` prefix, and make sure the `X-Lettermint-Signature` and `X-Lettermint-Delivery` headers reach the handler.
   - Rename types using the guide's type-name table. Enums are `#[non_exhaustive]`: add wildcard arms to `match` statements. Optional nullable fields are `Option<Option<T>>`.
5. Run `cargo build`, `cargo clippy --all-targets -- -D warnings` and `cargo test`, and fix every error. Don't send real email or call the live API while testing.
6. Finish with a summary: the files you changed, anything you could not migrate with certainty, and behaviour changes I should review.

Never print, log or commit API tokens or webhook secrets.
````

## Create the client

`Lettermint::email()`, `Lettermint::api()`, `Lettermint::email_with_transport()`, `Lettermint::api_with_transport()`, `EmailClient`, `ApiClient`, `HttpClient` and `AuthMode` are removed.

```rust,ignore
// 1.x
use lettermint::Lettermint;
let email = Lettermint::email(std::env::var("LETTERMINT_PROJECT_TOKEN")?)?;
let api = Lettermint::api(std::env::var("LETTERMINT_TEAM_TOKEN")?)?;
let test = Lettermint::email_with_transport("token", my_transport)?;

// 2.0
use lettermint::Lettermint;
let lettermint = Lettermint::builder()
    .sending_token(std::env::var("LETTERMINT_PROJECT_TOKEN")?) // for lettermint.emails()
    .team_token(std::env::var("LETTERMINT_TEAM_TOKEN")?) // for the Team API
    .timeout(std::time::Duration::from_secs(10)) // optional; default 30 s
    .build()?;
let test = Lettermint::builder().sending_token("lm_test").transport(my_transport).build()?;
```

Pass one token or both. With only one token, calling a part that needs the other returns `Error::Config` (``domains.list needs `team_token` ``) before any request.

You can also pass one token; the SDK chooses the kind by its prefix:

```rust,ignore
let lettermint = Lettermint::new("lm_team_...")?; // team token
let lettermint = Lettermint::new("lm_...")?; // project sending token
let lettermint = Lettermint::builder().token(token).base_url(url).build()?; // with options
```

Any other format (SSO tokens, OAuth tokens, an empty string) is an `Error::Config`. Use `sending_token()` or `team_token()` for those. In 1.x any non-empty string was accepted.

| 1.x | 2.0 |
| --- | --- |
| `Lettermint::email(token)` | `Lettermint::builder().sending_token(token).build()?`, then `.emails()` |
| `Lettermint::api(token)` | `Lettermint::builder().team_token(token).build()?` |
| `Lettermint::email_with_transport(token, t)` / `api_with_transport` | `Lettermint::builder()….transport(t).build()?` |
| `HttpClient::with_base_url(url)` | `Lettermint::builder().base_url(url)` |
| no timeout | `.timeout(duration)` on the builder (default 30 s) or `lettermint.with_timeout(duration)?` |
| `client::DEFAULT_BASE_URL`, `client::VERSION` | `lettermint::DEFAULT_BASE_URL`, `lettermint::VERSION` |

## Send an email

In 1.x, `client.email()` returned a builder that borrowed the client, `to`/`cc`/`bcc`/`reply_to` appended an address, and `send()` consumed the builder. In 2.0, `emails().compose()` returns an owned builder: setters still take it by value, the recipient setters replace the list (one address or a list), and `send()` borrows it, so a builder can be cloned as a template and sent again.

```rust,ignore
// 1.x
let email = Lettermint::email(token)?;
email
    .email()
    .from("Acme <hello@acme.com>")
    .to("jane@example.com")
    .to("john@example.com")
    .subject("Welcome")
    .html("<p>Hi</p>")
    .idempotency_key("welcome-1")
    .send()
    .await?;

// 2.0: builder
use lettermint::SendOptions;
lettermint
    .emails()
    .compose()
    .from("Acme <hello@acme.com>")
    .to(["jane@example.com", "john@example.com"])
    .subject("Welcome")
    .html("<p>Hi</p>")
    .send(SendOptions::new().idempotency_key("welcome-1"))
    .await?;

// 2.0: plain struct (API field names)
let message = lettermint::types::SendMailRequest {
    from: "Acme <hello@acme.com>".into(),
    to: vec!["jane@example.com".into()],
    subject: "Welcome".into(),
    html: Some(Some("<p>Hi</p>".into())),
    ..Default::default()
};
lettermint.emails().send(&message, SendOptions::new().idempotency_key("welcome-1")).await?;
```

### Changed builder methods

| 1.x | 2.0 |
| --- | --- |
| `client.email()` | `lettermint.emails().compose()` (or `compose_from(message)`) |
| `.to(x)`, `.cc(x)`, `.bcc(x)`, `.reply_to(x)` appended one address | Replace the list; take one address, an array, a `Vec` or a slice |
| `.header(name, value)` | `.headers([(name, value), …])`, replaces all headers |
| `.metadata(key, value)` | `.metadata([(key, value), …])`, replaces all metadata |
| `.tags([MessageTag::new(name, value)?, …])` | `.tags([(name, value), …])` or `MessageTagInput` values; validated by `send()` and `build()` |
| `.attach(filename, base64)` | `.attach(Attachment::from_base64(filename, base64))` |
| `.attach_with_options(filename, base64, content_id, content_type)` | `.attach(Attachment::from_base64(filename, base64).content_id(id).content_type(ct))`; `Attachment::from_bytes` encodes raw bytes |
| `.settings(EmailSettings { … })` | `.settings(SendMailRequestSettings { … })` |
| `.idempotency_key(key).send()` | `.send(SendOptions::new().idempotency_key(key))` |
| `.send()` consumed the builder | `.send(options)` borrows it; send again or `clone()` it as a template |
| — | `.clear_html()`, `.clear_text()`, `.clear_tag()`, `.clear_scheduled_at()`, `.build()`, `.message()` |

Unchanged: `from`, `subject`, `html`, `text`, `route`, `scheduled_at`, `tag`, `sandbox_result`.

In 1.x the builder skipped tag validation and `EmailClient::send` validated tags but had no idempotency key. In 2.0 every send validates tags first and takes `SendOptions`.

## Batch sending and ping

```rust,ignore
// 1.x
email.send_batch(&messages).await?;
email.send_batch_with_idempotency_key(&messages, "batch-1").await?;
email.ping().await?;
api.ping().await?;

// 2.0
lettermint.emails().send_batch(&messages, SendOptions::new()).await?;
lettermint.emails().send_batch(&messages, SendOptions::new().idempotency_key("batch-1")).await?;
lettermint.emails().send_batch(&[builder.build()?, other], SendOptions::new()).await?; // builders
lettermint.emails().ping().await?; // sending token
lettermint.ping().await?; // team token if configured, otherwise the sending token
```

A failed batch validation names the message: `Error::InvalidInput` with `field() == "messages[2].tags"`.

## Team API

The sub-clients move from `Lettermint::api(token)?.x()` to `lettermint.x()`. Query parameters are typed structs instead of `&[(&str, &str)]` with bracket keys, request bodies are typed (1.x accepted any `Serialize`), and every list has an `iterate()` method.

```rust,ignore
// 1.x
let api = Lettermint::api(token)?;
let page = api.domains().list(&[("page[size]", "10"), ("filter[status]", "verified")]).await?;

// 2.0
use lettermint::types::{DomainStatus, ListDomainsQuery};
let query = ListDomainsQuery { page_size: Some(10), filter_status: Some(DomainStatus::Verified), ..Default::default() };
let page = lettermint.domains().list(&query).await?;
let mut domains = lettermint.domains().iterate(&query);
while let Some(domain) = domains.next().await {
    println!("{}", domain?.domain);
}
```

| 1.x (`api = Lettermint::api(token)?`) | 2.0 (`lettermint` with a team token) |
| --- | --- |
| `api.ping()` | `lettermint.ping()` |
| `api.blocked_file_types()` | `lettermint.blocked_file_types()` |
| `api.analytics(&AnalyticsRequest)` | `lettermint.analytics(&AnalyticsQuery)` |
| `api.domains().list(&[..])` | `lettermint.domains().list(&ListDomainsQuery)`, `.iterate(&query)` |
| `api.domains().create(&payload)` | `lettermint.domains().create(&StoreDomainData)` |
| `api.domains().retrieve(id)` | `lettermint.domains().retrieve(id, &GetDomainQuery)` (`include: Some(vec![GetDomainQueryIncludeItem::DnsRecords])`) |
| `api.domains().delete(id)` | `lettermint.domains().delete(id)` |
| `api.domains().verify_dns_records(id)` | unchanged, on `lettermint.domains()` |
| `api.domains().verify_dns_record(id, record_id)` | unchanged, on `lettermint.domains()` |
| `api.domains().update_projects(id, &payload)` | `lettermint.domains().update_projects(id, &UpdateDomainProjectsData)` |
| `api.messages().list(&[..])` | `lettermint.messages().list(&ListMessagesQuery)`, `.iterate(&query)` |
| `api.messages().retrieve(id)` | `lettermint.messages().retrieve(id)` |
| `api.messages().events(id, &[..])` | `lettermint.messages().events(id, &ListMessageEventsQuery)`, `.iterate_events(id, &query)` |
| `api.messages().source(id)` / `.html(id)` / `.text(id)` | unchanged, on `lettermint.messages()` |
| `api.messages().reschedule(id, &payload)` | `lettermint.messages().reschedule(id, &RescheduleMessageRequest)` |
| `api.messages().cancel(id)` | `lettermint.messages().cancel(id)` |
| `api.messages().process(id)` | `lettermint.messages().process(id, SendOptions)`; no longer sends a `null` body |
| `api.projects().list(&[..])` | `lettermint.projects().list(&ListProjectsQuery)`, `.iterate(&query)` |
| `api.projects().create(&payload)` | `lettermint.projects().create(&StoreProjectData)` |
| `api.projects().retrieve(id)` | `lettermint.projects().retrieve(id, &GetProjectQuery)` |
| `api.projects().update(id, &payload)` | `lettermint.projects().update(id, &UpdateProjectData)` |
| `api.projects().delete(id)` | `lettermint.projects().delete(id)` |
| `api.projects().rotate_token(id)` | `lettermint.projects().rotate_token(id)` (deprecated by the API) |
| `api.projects().routes(project_id, &[..])` | `lettermint.routes().list(project_id, &ListRoutesQuery)`, `.iterate(project_id, &query)` |
| `api.projects().create_route(project_id, &payload)` | `lettermint.routes().create(project_id, &StoreRouteData::new(name, route_type))` |
| `api.projects().retrieve_report_forwarding(id)` | `lettermint.projects().report_forwarding().retrieve(id)` |
| `api.projects().update_report_forwarding(id, &payload)` | `lettermint.projects().report_forwarding().update(id, &payload)` |
| `api.projects().delete_report_forwarding(id)` | `lettermint.projects().report_forwarding().delete(id)` |
| `api.projects().verify_report_forwarding(id, &payload)` | `lettermint.projects().report_forwarding().verify(id, &payload)` |
| `api.projects().resend_report_forwarding_code(id)` | `lettermint.projects().report_forwarding().resend_code(id)` |
| `api.routes().retrieve(id)` | `lettermint.routes().retrieve(id, &GetRouteQuery)` |
| `api.routes().update(id, &payload)` | `lettermint.routes().update(id, &UpdateRouteData)` |
| `api.routes().delete(id)` | `lettermint.routes().delete(id)` |
| `api.routes().verify_inbound_domain(id)` | `lettermint.routes().verify_inbound_domain(id)` |
| `api.stats().retrieve(&[..])` | `lettermint.stats().retrieve(&GetStatsQuery { from, to, project_id, include_machine })` |
| `api.suppressions().list(&[..])` | `lettermint.suppressions().list(&ListSuppressionsQuery)`, `.iterate(&query)` |
| `api.suppressions().create(&payload)` | `lettermint.suppressions().create(&StoreSuppressionData::new(reason, scope))` |
| `api.suppressions().delete(id)` | `lettermint.suppressions().delete(id)` |
| `api.team().retrieve()` | `lettermint.team().retrieve(&GetTeamQuery)` (`include: Some(vec![GetTeamQueryIncludeItem::Features])`) |
| `api.team().update(&payload)` | `lettermint.team().update(&UpdateTeamData)` |
| `api.team().usage()` | `lettermint.team().usage()` |
| `api.team().roles()` | `lettermint.team().roles()` |
| `api.team().members(&[..])` | `lettermint.team().members().list(&ListTeamMembersQuery)`, `.iterate(&query)` |
| `api.team().member(user_id)` | `lettermint.team().members().retrieve(user_id)` |
| `api.team().update_member_assignment(user_id, &payload)` | `lettermint.team().members().update_assignment(user_id, &UpdateTeamMemberAssignmentData::new(role_id, access))` |
| `api.webhooks().list(&[..])` | `lettermint.webhooks().list(&ListWebhooksQuery)`, `.iterate(&query)` |
| `api.webhooks().create(&payload)` | `lettermint.webhooks().create(&StoreWebhookData)` |
| `api.webhooks().retrieve(id)` | `lettermint.webhooks().retrieve(id)` |
| `api.webhooks().update(id, &payload)` | `lettermint.webhooks().update(id, &UpdateWebhookData)` |
| `api.webhooks().delete(id)` | `lettermint.webhooks().delete(id)` |
| `api.webhooks().test(id)` | `lettermint.webhooks().test(id)` |
| `api.webhooks().regenerate_secret(id)` | `lettermint.webhooks().regenerate_secret(id)` |
| `api.webhooks().deliveries(id, &[..])` | `lettermint.webhooks().deliveries().list(id, &ListWebhookDeliveriesQuery)`, `.iterate(id, &query)` |
| `api.webhooks().delivery(id, delivery_id)` | `lettermint.webhooks().deliveries().retrieve(id, delivery_id)` |

`messages().reschedule()` and `messages().cancel()` accept either token: the team token when configured, otherwise the sending token. A sending-only client can cancel the scheduled email it sent.

### Query parameters

Each bracketed name is a snake_case field: `page[size]` is `page_size`, `filter[status]` is `filter_status`, `filter[startDate]` is `filter_start_date`. Value lists are sent comma-separated, object lists indexed, booleans as `1`/`0`.

| 1.x | 2.0 |
| --- | --- |
| `&[("page[size]", "30"), ("page[cursor]", c)]` | `ListDomainsQuery { page_size: Some(30), page_cursor: Some(c.into()), ..Default::default() }` |
| `&[("filter[status]", "verified")]` | `filter_status: Some(DomainStatus::Verified)` |
| `&[("sort", "-created_at,domain")]` | `sort: Some(vec![ListDomainsQuerySortItem::CreatedAtDesc, ListDomainsQuerySortItem::Domain])` |
| `&[("filter[tags][0][name]", "a"), ("filter[tags][0][value]", "b")]` | `filter: Some(ListMessagesQueryFilter { tags: Some(vec![ListMessagesQueryFilterTagsItem { name: Some("a".into()), value: Some("b".into()) }]) })` |
| `&[("filter[enabled]", "true")]` | `filter_enabled: Some(true)` |
| webhooks: `&[("cursor", c)]` | `ListWebhooksQuery { cursor: Some(c.into()), .. }` (these lists use `cursor`, not `page[cursor]`) |

### Path parameters

IDs are URL-encoded as before (like JavaScript's `encodeURIComponent`). An empty ID, `.` or `..` is now an `Error::Config`, returned before the request.

### Lists

1.x list responses had `data: Vec<Box<T>>`, `links` and `meta: serde_json::Value`. In 2.0 every list is `CursorPage<T>` with `data: Vec<T>`, `next_cursor`, `prev_cursor`, `per_page`, `path`, `next_page_url` and `prev_page_url`. Or use `iterate()`, which returns a `Paginator` (inherent `next()` and `futures::Stream`).

### Removed

`endpoints::OPERATION_IDS` is removed; every documented operation has a method. The endpoint structs `Domains<'a>` … `Webhooks<'a>` in `lettermint::endpoints` became owned sub-clients in `lettermint::resources`, returned by the client's methods.

## Errors

Every error is `lettermint::Error`. The enum is `#[non_exhaustive]`; keep a wildcard arm.

| Situation | 1.x | 2.0 |
| --- | --- | --- |
| HTTP 401 / 403 / 404 / 409 | `Error::Http { status, message, body }` | `Error::Authentication` / `Permission` / `NotFound` / `Conflict` (`ApiError`) |
| HTTP 422 | `Error::Validation { error_type, body }` (body not redacted) | `Error::Validation(ApiError)`: `code()`, `message()`, `errors()`, `body()`, redacted |
| HTTP 429 | `Error::Http` | `Error::RateLimit(ApiError)`: `retry_after()` |
| HTTP 5xx | `Error::Http` | `Error::Server(ApiError)` |
| Other 4xx | `Error::Http` | `Error::Api(ApiError)` |
| Redirect (3xx) | followed, with the token | `Error::Redirect { status }`; never followed |
| Empty, invalid or mismatching JSON body; HTML error page | `Error::Json` or `Error::Http` | `Error::UnexpectedResponse` (`status()`, `body_excerpt()`) |
| Timeout | none (requests could hang) | `Error::Timeout { timeout }`; default 30 s, covers the body |
| Network failure | `Error::Request(reqwest::Error)` | `Error::Connection { source }` |
| Invalid tags | `Error::InvalidMessageTag(String)` | `Error::InvalidInput(InputError)` (`field()`, `message()`) |
| Missing or wrong token, bad option | `Error::MissingToken` | `Error::Config(ConfigError)` |
| Invalid header value | `Error::InvalidHeader` | `Error::Config` (token) or `Error::InvalidInput` (idempotency key) |
| Webhook failures | `InvalidSignature`, `InvalidSignatureFormat`, `TimestampMismatch`, `TimestampOutsideTolerance`, `MissingWebhookSecret`, `InvalidWebhookJson`, `InvalidHmacKey` | `WebhookVerificationError` with `reason()` (see [Webhooks](#webhooks)); an empty secret is `Error::Config` |

`Error::status()` and `Error::code()` work across variants; `Error::api_error()` returns the `ApiError` of any HTTP variant.

```rust,ignore
// 1.x
match email.send(&payload).await {
    Err(Error::Validation { error_type, body }) => eprintln!("{error_type} {body:?}"),
    Err(Error::Http { status: 429, .. }) => retry_later(),
    other => other.map(drop)?,
}

// 2.0
match lettermint.emails().send(&payload, SendOptions::new().idempotency_key(key)).await {
    Ok(_) => {}
    Err(Error::Validation(error)) => eprintln!("{} {:?}", error.message(), error.errors()),
    Err(Error::RateLimit(error)) => retry_later(error.retry_after()),
    Err(error) => return Err(error.into()),
}
```

The SDK does not retry. Pass an idempotency key when you retry a send. To cancel a request, drop its future (for example with `tokio::time::timeout`).

## Webhooks

```rust,ignore
// 1.x
let webhook = Webhook::new(secret).with_tolerance(300);
let payload: serde_json::Value = webhook.verify(body, signature, Some(delivery_timestamp))?;
let payload: serde_json::Value = webhook.verify_headers(body, &headers)?; // BTreeMap<String, String>

// 2.0
let webhook = Webhook::new(secret)?.with_tolerance(std::time::Duration::from_secs(300));
let payload = webhook.verify_signature(body, signature, Some(delivery_header))?; // WebhookPayload
let payload = webhook.verify(body, &headers)?; // http::HeaderMap, HashMap, BTreeMap or (name, value) pairs
println!("{} {}", payload.event, payload.data);
```

- `Webhook::new` returns a `Result`: an empty secret is an `Error::Config`. `with_tolerance` takes a `Duration`.
- `verify(body, headers)` replaces `verify_headers(body, headers)` and takes `http::HeaderMap` too. The old `verify(body, signature, delivery)` is `verify_signature(body, signature, delivery)`, with the delivery header as a `&str`.
- The body may be a `&str`, `String`, `&[u8]` or `Vec<u8>`; pass it exactly as received.
- Both `X-Lettermint-Signature` and `X-Lettermint-Delivery` are required by `verify`, and the delivery header must equal the signed timestamp.
- Any `v1` signature in the header may match (1.x checked only the last one). A duplicate `t` or a non-ASCII header is a `signature_header_malformed` error, never a panic.
- Errors are `WebhookVerificationError` with a `reason()`: `signature_header_missing`, `signature_header_malformed`, `delivery_header_missing`, `delivery_timestamp_mismatch`, `timestamp_out_of_tolerance`, `signature_mismatch`, `body_invalid` or `payload_invalid`. It converts into `lettermint::Error` with `?`.
- The result is a `WebhookPayload` (`event: WebhookEvent`, `data`, `id`, `timestamp`, `extra`) instead of `serde_json::Value`. `payload.data_as::<T>()` decodes `data`.
- `DEFAULT_TOLERANCE_SECONDS: i64` is `DEFAULT_TOLERANCE: Duration`. `Webhook`'s `Debug` output no longer shows the secret.

## Custom transports

`Transport` stays an `async_trait` trait with one method, but its types changed:

| 1.x | 2.0 |
| --- | --- |
| `HttpRequest { method: String, url, headers: BTreeMap<String, String>, body: Option<String> }` | `HttpRequest { method: http::Method, url, headers: http::HeaderMap, body: Option<Vec<u8>>, timeout }` |
| `HttpResponse { status, reason, body: String }` | `HttpResponse { status, headers: http::HeaderMap, body: Vec<u8> }` (`HttpResponse::new(status, headers, body)`) |
| return `Error::Request(..)` | return `Error::connection(source)` or `Error::timeout(duration)` |
| `ReqwestTransport::new()` followed redirects | `ReqwestTransport::new()` and `ReqwestTransport::from_builder(reqwest::ClientBuilder)` never follow redirects |

A transport must not follow redirects or retry. `HttpRequest`'s `Debug` output redacts the token headers.

## Type names

The types are generated from the API specification of lettermint#2582 and use its names. Types that are not listed below keep their name. Some shapes also changed:

- Enums are open and `#[non_exhaustive]`: an unknown value is `Other(String)` and serializes back unchanged. Add a wildcard arm to every `match`. Enums no longer implement `Default`.
- Required fields are required: `SendMailResponse::message_id` is a `String` (it was `Option<String>`), and a response without a required field is `Error::UnexpectedResponse` instead of a default value. Structs no longer use `#[serde(default)]`.
- Optional nullable fields are `Option<Option<T>>`, for example `SendMailRequest::html`, `tag` and `text`: `None` leaves the field out, `Some(None)` sends `null`. Required nullable fields are `Option<T>`.
- Nested fields are no longer boxed (`Vec<T>`, not `Vec<Box<T>>`).
- `SendMailRequest::attachments` is `Vec<MessageAttachmentInput>` and `settings` is `SendMailRequestSettings` (they were `serde_json::Value`).
- Request structs with required enum fields have no `Default`; use their `new(..)` constructor (`StoreRouteData::new`, `StoreSuppressionData::new`, `UpdateTeamMemberAssignmentData::new`, `AnalyticsFilter::new`, `AnalyticsSort::new`).
- The 422 and structured error bodies are `ValidationErrorBody` and `ApiErrorBody`, because `ApiError` is the SDK's error type.

### Renamed types

Every 1.x type in `lettermint::types` whose name changed. Several 1.x types now share one name, because the API uses one schema for them.

| `AnalyticsRequest` | `AnalyticsQuery` |
| `AnalyticsRequestFiltersItem` | `AnalyticsFilter` |
| `AnalyticsRequestSort` | `AnalyticsSort` |
| `AnalyticsResponseMeta` | `AnalyticsMeta` |
| `AnalyticsResponseMetaComparison` | `AnalyticsMetaComparison` |
| `AnalyticsResponsePagination` | `AnalyticsPagination` |
| `AnalyticsResponsePayload` | `AnalyticsResults` |
| `AnalyticsResponsePayloadBreakdownItem` | `AnalyticsBreakdownRow` |
| `AnalyticsResponsePayloadSummary` | `AnalyticsSummary` |
| `AnalyticsResponsePayloadTimeSeriesItem, AnalyticsResponsePayloadBreakdownItemTrendItem` | `AnalyticsTimeSeriesPoint` |
| AnalyticsResponsePayload…Metrics, …PreviousMetrics (8 classes) | `AnalyticsMetricValues` |
| AnalyticsResponsePayload…Previous (4 classes) | `AnalyticsComparisonValues` |
| AnalyticsResponsePayload…RateBases, …PreviousRateBases (8 classes) | `AnalyticsRateBases` |
| AnalyticsResponsePayload…RateBases…Rate (56 classes, for example AnalyticsResponsePayloadSummaryRateBasesBounceRate) | `AnalyticsRateBase` |
| `BlockedFileTypesResponse` | `BlockedFileTypes` |
| `CancelScheduledMessageResponse` | `ScheduledMessage` |
| `CursorPaginator` | `CursorPage<T>` |
| `DomainDestroyResponse` | `MessageResponse` |
| `DomainIndexResponse` | `CursorPage<DomainListData>` |
| `DomainUpdateProjectsResponse` | `DomainMutationResponse` |
| `DomainVerifyDnsRecordsResponse` | `DnsVerificationSuccessResponse` |
| `DomainVerifySpecificDnsRecordResponse` | `MessageResponse` |
| `MessageEventsResponse` | `CursorPage<MessageEventData>` |
| `MessageIndexResponse` | `CursorPage<MessageListData>` |
| `ProjectDestroyResponse` | `MessageResponse` |
| `ProjectIndexResponse` | `CursorPage<ProjectListData>` |
| `ProjectRotateTokenResponse` | `RotateProjectTokenResponse` |
| `ProjectStoreResponse` | `ProjectCreatedData` |
| `ProjectUpdateResponse` | `ProjectMutationResponse` |
| `RescheduleMessageResponse` | `ScheduledMessage` |
| `RouteDestroyResponse` | `MessageResponse` |
| `RouteIndexResponse` | `CursorPage<RouteListData>` |
| `RouteStoreResponse` | `RouteMutationResponse` |
| `RouteUpdateResponse` | `RouteMutationResponse` |
| `RouteVerifyInboundDomainResponse` | `InboundDomainVerificationResponse` |
| `SuppressionDestroyResponse` | `DeleteSuppressionResponse` |
| `SuppressionIndexResponse` | `CursorPage<SuppressedRecipientData>` |
| `TeamMembersResponse` | `CursorPage<TeamMemberData>` |
| `TeamRolesResponse` | `TeamRoleListResponse` |
| `TeamUpdateResponse` | `TeamMutationResponse` |
| `UpdateReportForwardingRequest` | `ReportForwardingRequest` |
| `WebhookDeliveriesResponse` | `CursorPage<WebhookDeliveryListData>` |
| `WebhookDestroyResponse` | `MessageResponse` |
| `WebhookIndexResponse` | `CursorPage<WebhookListData>` |
| `WebhookRegenerateSecretResponse` | `WebhookSecretResponse` |
| `WebhookStoreResponse` | `WebhookSecretResponse` |
| `WebhookTestResponse` | `TestWebhookResponse` |
| `WebhookUpdateResponse` | `WebhookMutationResponse` |
### Removed types

lettermint#2582 removed these schemas from the API specification:

| 1.x | 2.0 |
| --- | --- |
| `AnalyticsResponseData` | Removed. Use `AnalyticsResponse` (`data: AnalyticsResults`). |
| `StatsRequestData` | Removed. Use `GetStatsQuery`, the parameter of `stats().retrieve()`. |
| Message list `meta` (`MessageIndexResponseMeta`) | Removed; 1.x typed it as `serde_json::Value`. Lists are `CursorPage<T>`. |
| Message events `meta` (`MessageEventsResponseMeta`) | Removed; 1.x typed it as `serde_json::Value`. |
| `SuppressionStoreResponseMessage1` | Removed; not exported by 1.x. `SuppressionStoreResponse::message` is a `String`. |

### Removed and moved exports

| 1.x | 2.0 |
| --- | --- |
| `Lettermint::email`, `Lettermint::api`, `Lettermint::email_with_transport`, `Lettermint::api_with_transport` | `Lettermint::builder()` / `Lettermint::new()` (see [Create the client](#create-the-client)) |
| `ApiClient`, `EmailClient`, `client::HttpClient`, `client::AuthMode` | `Lettermint` and `Lettermint::emails()` (`Emails`) |
| `EmailBuilder<'a>` | `EmailBuilder` (owned, from `emails().compose()`) |
| `EmailSettings` | `types::SendMailRequestSettings` |
| `types::MessageTag::new(name, value)?` | `(name, value)` pairs or `types::MessageTagInput`; validated before the request |
| `types::EmailAttachment` | `Attachment` (builder) or `types::MessageAttachmentInput` |
| `endpoints::{Domains, Messages, Projects, Routes, Stats, Suppressions, Team, Webhooks}<'a>` | `resources::{Domains, Messages, Projects, ReportForwarding, Routes, Stats, Suppressions, Team, TeamMembers, Webhooks, WebhookDeliveries}` |
| `endpoints::OPERATION_IDS` | Removed. |
| `client::{Transport, ReqwestTransport, HttpRequest, HttpResponse}` | `lettermint::{Transport, ReqwestTransport, HttpRequest, HttpResponse}` (see [Custom transports](#custom-transports)) |
| `client::DEFAULT_BASE_URL`, `client::VERSION` | `lettermint::DEFAULT_BASE_URL`, `lettermint::VERSION` |
| `webhook::{SIGNATURE_HEADER, DELIVERY_HEADER}` | `lettermint::{SIGNATURE_HEADER, DELIVERY_HEADER}` |
| `webhook::DEFAULT_TOLERANCE_SECONDS` | `lettermint::DEFAULT_TOLERANCE` (a `Duration`) |
| `Webhook::verify_headers`, `Webhook::verify(body, signature, delivery)` | `Webhook::verify(body, headers)`, `Webhook::verify_signature(body, signature, delivery)` |
| `Error::{MissingToken, InvalidMessageTag, Http, Validation { .. }, Request, InvalidHeader, Json, InvalidSignature, InvalidSignatureFormat, TimestampMismatch, TimestampOutsideTolerance, MissingWebhookSecret, InvalidWebhookJson, InvalidHmacKey}` | See [Errors](#errors) |
| The modules `client`, `email`, `endpoints`, `error`, `webhook` | Items are exported from the crate root; `types` and `resources` stay modules |

# Upgrade from 0.3 to 1.0


Version 1.0 changes the public client interface. Cargo does not select 1.0 for a dependency that uses `lettermint = "0.3"`. Update the version requirement only when the application is ready for these changes.

## Send email

Version 0.3 uses a request value and `Query::execute`:

```rust
use lettermint::api::email::SendEmailRequest;
use lettermint::reqwest::LettermintClient;
use lettermint::Query;

let client = LettermintClient::new("project-token");
let request = SendEmailRequest::builder()
    .from("sender@example.com")
    .to(vec!["recipient@example.com".into()])
    .subject("Hello")
    .text("Hello from Lettermint")
    .build();
request.execute(&client).await?;
```

Version 1.0 uses `Lettermint::email` and the email builder:

```rust
use lettermint::Lettermint;

let client = Lettermint::email("project-token")?;
client
    .email()
    .from("sender@example.com")
    .to("recipient@example.com")
    .subject("Hello")
    .text("Hello from Lettermint")
    .send()
    .await?;
```

## Send a batch

Use `EmailClient::send_batch` with a slice of `types::SendMailRequest` values. Batch validation is now handled by the API client.

## Team API

Create a Team API client with `Lettermint::api("team-token")`. Access resources through methods such as `domains()`, `messages()`, `projects()`, and `webhooks()`.

## Custom HTTP transport

Implement `client::Transport` and use `Lettermint::email_with_transport` or `Lettermint::api_with_transport`. The old `Client`, `Endpoint`, and `Query` traits are not part of the 1.0 interface.

## Errors

Version 1.0 returns `lettermint::Error`. Match its variants for transport, HTTP, API, serialization, webhook, and local validation failures.

## Webhooks

`Webhook::new` now returns a verifier directly. Pass the optional delivery timestamp to `verify`, or use `verify_headers` when all Lettermint webhook headers are available.
