//! The official Rust SDK for [Lettermint](https://lettermint.co): send email and manage your
//! team through the Lettermint API.
//!
//! ```no_run
//! use lettermint::{Lettermint, SendOptions};
//!
//! # async fn run() -> lettermint::Result<()> {
//! let lettermint = Lettermint::builder()
//!     .sending_token(std::env::var("LETTERMINT_PROJECT_TOKEN").unwrap_or_default())
//!     .build()?;
//!
//! let response = lettermint
//!     .emails()
//!     .compose()
//!     .from("Acme <hello@acme.com>")
//!     .to("jane@example.com")
//!     .subject("Welcome to Acme")
//!     .html("<p>Thanks for signing up.</p>")
//!     .send(SendOptions::new().idempotency_key("welcome-jane"))
//!     .await?;
//! println!("{} {}", response.message_id, response.status);
//! # Ok(())
//! # }
//! ```
//!
//! - [`Lettermint`] is the client. [`Lettermint::emails`] uses the project sending token; the
//!   Team API ([`Lettermint::domains`], [`Lettermint::messages`], …) uses the team token.
//! - [`types`] holds the request and response types, generated from the API specification.
//! - [`Error`] is the error type; [`Webhook`] verifies webhook deliveries.
//!
//! See the [README](https://github.com/lettermint/lettermint-rust#readme) for a guide and
//! `UPGRADE.md` for upgrading from 1.x.

#![warn(missing_docs)]
#![deny(unsafe_code)]

mod client;
mod emails;
mod error;
#[rustfmt::skip]
#[allow(missing_docs)]
mod generated;
mod pagination;
mod query;
pub mod resources;
mod tokens;
mod transport;
mod webhook;

pub use client::{Lettermint, LettermintBuilder};
pub use emails::{Attachment, EmailBuilder, Emails, IntoAddresses, SendOptions};
pub use error::{
    ApiError, BoxError, ConfigError, Error, InputError, Result, UnexpectedResponseError,
};
pub use pagination::Paginator;
pub use tokens::TokenKind;
pub use transport::{
    DEFAULT_BASE_URL, DEFAULT_TIMEOUT, HttpRequest, HttpResponse, ReqwestTransport, Transport,
};
pub use webhook::{
    DEFAULT_TOLERANCE, DELIVERY_HEADER, SIGNATURE_HEADER, Webhook, WebhookHeaders, WebhookPayload,
    WebhookVerificationError, WebhookVerificationReason,
};

/// Request, response and query types of the Lettermint API, generated from its specification.
///
/// - Structs keep optional and nullable apart: `Option<T>` is an optional field (left out when
///   `None`), a required nullable field is `Option<T>` that the API always sends, and an optional
///   nullable field is `Option<Option<T>>` (`None` leaves it out, `Some(None)` sends `null`).
///   A response without a required field is an [`Error::UnexpectedResponse`].
/// - Enums are open and `#[non_exhaustive]`: a value the API adds later decodes as `Other(String)`
///   and serializes back unchanged. Match with a wildcard arm.
/// - Lists are [`CursorPage<T>`](types::CursorPage).
/// - New fields may be added in minor releases. Build request structs with
///   `..Default::default()` where the type implements `Default`.
pub mod types {
    pub use crate::generated::types::*;
}

/// The version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests;

/// Compiles the README examples as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
