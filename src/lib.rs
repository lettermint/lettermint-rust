pub mod client;
pub mod email;
pub mod endpoints;
pub mod error;
pub mod types;
pub mod webhook;

pub use client::{ApiClient, EmailClient, Lettermint};
pub use email::{EmailBuilder, EmailSettings};
pub use error::{Error, Result};
pub use webhook::Webhook;
