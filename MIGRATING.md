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
