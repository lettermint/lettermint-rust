# Lettermint Rust SDK

[![Crates.io Version](https://img.shields.io/crates/v/lettermint?style=flat-square)](https://crates.io/crates/lettermint)
[![Crates.io Downloads](https://img.shields.io/crates/d/lettermint?style=flat-square)](https://crates.io/crates/lettermint)
[![docs.rs](https://img.shields.io/docsrs/lettermint?style=flat-square)](https://docs.rs/lettermint)
[![GitHub Tests](https://img.shields.io/github/actions/workflow/status/lettermint/lettermint-rust/ci.yml?branch=main&label=tests&style=flat-square)](https://github.com/lettermint/lettermint-rust/actions?query=workflow%3ACI+branch%3Amain)
[![Dependency Status](https://deps.rs/crate/lettermint/latest/status.svg)](https://deps.rs/crate/lettermint)
[![License](https://img.shields.io/github/license/lettermint/lettermint-rust?style=flat-square)](https://github.com/lettermint/lettermint-rust/blob/main/LICENSE)
[![Join our Discord server](https://img.shields.io/discord/1305510095588819035?logo=discord&logoColor=eee&label=Discord&labelColor=464ce5&color=0D0E28&cacheSeconds=43200)](https://lettermint.co/r/discord)

Official Rust SDK for the Lettermint sending and team APIs.

## Requirements

- Current stable Rust. The crate's `rust-version` tracks the current supported stable minor release.
- Tokio or another async runtime compatible with `reqwest`

## Installation

```toml
[dependencies]
lettermint = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Send Email

```rust
use lettermint::Lettermint;

#[tokio::main]
async fn main() -> lettermint::Result<()> {
    let email = Lettermint::email(std::env::var("LETTERMINT_TOKEN").unwrap())?;

    let response = email
        .email()
        .from("sender@example.com")
        .to("recipient@example.com")
        .subject("Hello from Rust")
        .html("<p>Hello from Lettermint.</p>")
        .idempotency_key("welcome-123")
        .send()
        .await?;

    println!("{}", response.message_id);
    Ok(())
}
```

The fluent email builder owns its payload. Each call to `email.email()` starts with a fresh payload, so attachments, headers, metadata, and recipients do not leak between sends.

## Direct API Payloads

```rust
use lettermint::{types, Lettermint};

#[tokio::main]
async fn main() -> lettermint::Result<()> {
    let email = Lettermint::email(std::env::var("LETTERMINT_TOKEN").unwrap())?;

    let payload = types::SendMailRequest {
        from: "sender@example.com".into(),
        to: vec!["recipient@example.com".into()],
        subject: "Typed payload".into(),
        text: Some("Hello from Lettermint.".into()),
        ..Default::default()
    };

    email.send(&payload).await?;
    Ok(())
}
```

## Team API

```rust
use lettermint::Lettermint;

#[tokio::main]
async fn main() -> lettermint::Result<()> {
    let api = Lettermint::api(std::env::var("LETTERMINT_TEAM_TOKEN").unwrap())?;
    let domains = api.domains().list(&[("page[size]", "10")]).await?;

    for domain in domains.data {
        println!("{}", domain.domain);
    }

    Ok(())
}
```

`Lettermint::email(...)` authenticates with `X-Lettermint-Token` for sending endpoints. `Lettermint::api(...)` authenticates with `Authorization: Bearer ...` for team endpoints.

## Webhooks

```rust
use lettermint::Webhook;

fn handle(payload: &str, signature: &str, delivery: i64) -> lettermint::Result<serde_json::Value> {
    Webhook::new(std::env::var("LETTERMINT_WEBHOOK_SECRET").unwrap())
        .verify(payload, signature, Some(delivery))
}
```

Webhook verification checks the `t=...` timestamp, validates the `v1=...` HMAC-SHA256 signature in constant time, cross-checks the optional delivery timestamp, and enforces a 5 minute tolerance by default.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --locked
```

Generated DTOs live in `src/types.rs`. Regenerate them from the repository root with:

```bash
python3 sdk-generator/rust/generate-types.py
```
