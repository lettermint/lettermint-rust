//! The [`Lettermint`] client and its builder.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use crate::emails::Emails;
use crate::error::{ConfigError, Result};
use crate::generated::operations::{self as ops, Endpoint, PaginatedEndpoint};
use crate::generated::support::QueryParams;
use crate::generated::types::{AnalyticsQuery, AnalyticsResponse, BlockedFileTypes};
use crate::pagination::Paginator;
use crate::resources::{Domains, Messages, Projects, Routes, Stats, Suppressions, Team, Webhooks};
use crate::tokens::{REDACTED, Secret, TokenKind, check_token};
use crate::transport::{
    CallOptions, DEFAULT_BASE_URL, DEFAULT_TIMEOUT, ReqwestTransport, Transport,
};

/// The Lettermint client.
///
/// ```no_run
/// # fn main() -> lettermint::Result<()> {
/// use lettermint::Lettermint;
///
/// // One token: the SDK detects its kind by the prefix (`lm_team_…` or `lm_…`).
/// let lettermint = Lettermint::new("lm_...")?;
///
/// // Both tokens, with options.
/// let lettermint = Lettermint::builder()
///     .sending_token("lm_...")
///     .team_token("lm_team_...")
///     .timeout(std::time::Duration::from_secs(10))
///     .build()?;
/// # Ok(())
/// # }
/// ```
///
/// [`emails`](Self::emails) uses the sending token; every other part uses the team token. The
/// client holds no message state, is cheap to clone (it shares one connection pool) and is safe
/// to share across tasks. Its `Debug` output shows tokens as `[redacted]`.
#[derive(Clone)]
pub struct Lettermint {
    core: Arc<Core>,
    timeout: Duration,
}

pub(crate) struct Core {
    pub(crate) sending_token: Option<Secret>,
    pub(crate) team_token: Option<Secret>,
    pub(crate) base_url: String,
    pub(crate) transport: Arc<dyn Transport>,
}

impl Lettermint {
    /// A client from one token. `lm_team_` followed by letters and digits is a team token, `lm_`
    /// followed by letters and digits a sending token. Any other format, such as an SSO token
    /// (`lm_sso_…`), is an [`Error::Config`](crate::Error::Config); use
    /// [`builder`](Self::builder) with [`sending_token`](LettermintBuilder::sending_token) or
    /// [`team_token`](LettermintBuilder::team_token) for it.
    pub fn new(token: impl Into<String>) -> Result<Self> {
        Self::builder().token(token).build()
    }

    /// A builder for a client with both tokens or other options.
    pub fn builder() -> LettermintBuilder {
        LettermintBuilder::default()
    }

    /// A copy of this client with another request timeout. It shares the connection pool.
    ///
    /// Returns an [`Error::Config`](crate::Error::Config) when `timeout` is zero.
    pub fn with_timeout(&self, timeout: Duration) -> Result<Self> {
        check_timeout(timeout)?;
        Ok(Self {
            core: Arc::clone(&self.core),
            timeout,
        })
    }

    /// The API base URL.
    pub fn base_url(&self) -> &str {
        &self.core.base_url
    }

    /// The request timeout. It covers the whole request, including reading the body.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Whether a sending token is configured.
    pub fn has_sending_token(&self) -> bool {
        self.core.sending_token.is_some()
    }

    /// Whether a team token is configured.
    pub fn has_team_token(&self) -> bool {
        self.core.team_token.is_some()
    }

    /// Send email. Needs the sending token.
    pub fn emails(&self) -> Emails {
        Emails::new(self.clone())
    }

    /// Sending domains. Needs the team token.
    pub fn domains(&self) -> Domains {
        Domains::new(self.clone())
    }

    /// Sent and received messages. Needs the team token; `reschedule` and `cancel` also accept
    /// the sending token.
    pub fn messages(&self) -> Messages {
        Messages::new(self.clone())
    }

    /// Projects and their report forwarding. Needs the team token.
    pub fn projects(&self) -> Projects {
        Projects::new(self.clone())
    }

    /// Routes of a project. Needs the team token.
    pub fn routes(&self) -> Routes {
        Routes::new(self.clone())
    }

    /// Sending statistics. Needs the team token.
    pub fn stats(&self) -> Stats {
        Stats::new(self.clone())
    }

    /// The suppression list. Needs the team token.
    pub fn suppressions(&self) -> Suppressions {
        Suppressions::new(self.clone())
    }

    /// The team and its members. Needs the team token.
    pub fn team(&self) -> Team {
        Team::new(self.clone())
    }

    /// Webhook endpoints and their deliveries. Needs the team token. To verify incoming
    /// deliveries, use [`Webhook`](crate::Webhook).
    pub fn webhooks(&self) -> Webhooks {
        Webhooks::new(self.clone())
    }

    /// Checks the configured token: `GET /ping` returns `pong`. Uses the team token when
    /// configured, otherwise the sending token.
    pub async fn ping(&self) -> Result<String> {
        let text: String = self
            .call::<ops::Ping>("ping", &[], &(), None, CallOptions::default())
            .await?;
        Ok(text.trim().to_owned())
    }

    /// Queries email analytics. Needs the team token.
    pub async fn analytics(&self, query: &AnalyticsQuery) -> Result<AnalyticsResponse> {
        self.call::<ops::QueryAnalytics>("analytics", &[], &(), Some(query), CallOptions::default())
            .await
    }

    /// The file extensions and MIME types that cannot be attached. Needs the team token.
    pub async fn blocked_file_types(&self) -> Result<BlockedFileTypes> {
        self.call::<ops::ListBlockedFileTypes>(
            "blocked_file_types",
            &[],
            &(),
            None,
            CallOptions::default(),
        )
        .await
    }

    pub(crate) fn core(&self) -> &Core {
        &self.core
    }

    /// Calls operation `E`, with its query, request and response types from the operation table.
    pub(crate) async fn call<E: Endpoint>(
        &self,
        label: &str,
        path: &[&str],
        query: &E::Query,
        body: Option<&E::Request>,
        options: CallOptions,
    ) -> Result<E::Response> {
        self.execute::<E::Response, E::Request>(
            E::OPERATION,
            label,
            path,
            &query.query_params(),
            body,
            options,
        )
        .await
    }

    /// Calls operation `E` (without query parameters) with a body of another serializable type,
    /// for example a slice instead of a `Vec`.
    pub(crate) async fn call_with<E: Endpoint, B: Serialize + ?Sized>(
        &self,
        label: &str,
        path: &[&str],
        body: &B,
        options: CallOptions,
    ) -> Result<E::Response> {
        self.execute::<E::Response, B>(E::OPERATION, label, path, &[], Some(body), options)
            .await
    }

    /// Follows `next_cursor` through every page of the list operation `E`.
    pub(crate) fn paginate<E>(
        &self,
        label: &'static str,
        path: &[&str],
        query: &E::Query,
    ) -> Paginator<E::Item>
    where
        E: PaginatedEndpoint,
        E::Item: Send + 'static,
    {
        let client = self.clone();
        let path: Vec<String> = path.iter().map(|segment| (*segment).to_owned()).collect();
        let params = query.query_params();
        Paginator::new(move |cursor: Option<String>| {
            let client = client.clone();
            let path = path.clone();
            let mut params = params.clone();
            Box::pin(async move {
                if let Some(cursor) = cursor {
                    let name = E::OPERATION
                        .pagination
                        .and_then(|pagination| pagination.cursor_param)
                        .expect("paginated operations have a cursor parameter");
                    crate::query::set_param(&mut params, name, &cursor);
                }
                let path: Vec<&str> = path.iter().map(String::as_str).collect();
                client
                    .execute::<crate::generated::types::CursorPage<E::Item>, ()>(
                        E::OPERATION,
                        label,
                        &path,
                        &params,
                        None,
                        CallOptions::default(),
                    )
                    .await
            })
        })
    }
}

impl fmt::Debug for Lettermint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Lettermint")
            .field("base_url", &self.core.base_url)
            .field("timeout", &self.timeout)
            .field(
                "sending_token",
                &self.core.sending_token.as_ref().map(|_| REDACTED),
            )
            .field(
                "team_token",
                &self.core.team_token.as_ref().map(|_| REDACTED),
            )
            .finish()
    }
}

/// Builds a [`Lettermint`] client. Pass at least one token.
#[derive(Default)]
pub struct LettermintBuilder {
    token: Option<String>,
    sending_token: Option<String>,
    team_token: Option<String>,
    base_url: Option<String>,
    timeout: Option<Duration>,
    transport: Option<Arc<dyn Transport>>,
}

impl LettermintBuilder {
    /// A token whose kind the SDK detects by its format, as in [`Lettermint::new`].
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// A project sending token (`lm_…`), sent as `x-lettermint-token`. Used by
    /// [`Lettermint::emails`].
    pub fn sending_token(mut self, token: impl Into<String>) -> Self {
        self.sending_token = Some(token.into());
        self
    }

    /// A team API token (`lm_team_…`), sent as `Authorization: Bearer`. Used by the Team API.
    pub fn team_token(mut self, token: impl Into<String>) -> Self {
        self.team_token = Some(token.into());
        self
    }

    /// The API base URL. Default `https://api.lettermint.co/v1`.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// The request timeout, covering the whole request including the body. Default 30 seconds.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// The HTTP transport. Default: [`ReqwestTransport`]. A custom transport must not follow
    /// redirects (see [`Transport`]).
    pub fn transport(mut self, transport: impl Transport) -> Self {
        self.transport = Some(Arc::new(transport));
        self
    }

    /// Builds the client. Returns an [`Error::Config`](crate::Error::Config) when no token is
    /// set, a token or option is invalid, or `token` conflicts with an explicit token.
    pub fn build(self) -> Result<Lettermint> {
        let mut sending_token = self
            .sending_token
            .map(|token| check_token("sending_token", token))
            .transpose()?;
        let mut team_token = self
            .team_token
            .map(|token| check_token("team_token", token))
            .transpose()?;
        if let Some(token) = self.token {
            let slot = match TokenKind::detect(&token)? {
                TokenKind::Sending => &mut sending_token,
                TokenKind::Team => &mut team_token,
            };
            if slot.is_some() {
                return Err(ConfigError::new(
                    "Pass a token either with token() or with sending_token()/team_token(), not both.",
                )
                .into());
            }
            *slot = Some(Secret(token));
        }
        if sending_token.is_none() && team_token.is_none() {
            return Err(ConfigError::new("Pass a sending token, a team token or both.").into());
        }
        let base_url = check_base_url(self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?;
        let timeout = self.timeout.unwrap_or(DEFAULT_TIMEOUT);
        check_timeout(timeout)?;
        let transport = match self.transport {
            Some(transport) => transport,
            None => Arc::new(ReqwestTransport::new()?),
        };
        Ok(Lettermint {
            core: Arc::new(Core {
                sending_token,
                team_token,
                base_url,
                transport,
            }),
            timeout,
        })
    }
}

impl fmt::Debug for LettermintBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LettermintBuilder")
            .field("token", &self.token.as_ref().map(|_| REDACTED))
            .field(
                "sending_token",
                &self.sending_token.as_ref().map(|_| REDACTED),
            )
            .field("team_token", &self.team_token.as_ref().map(|_| REDACTED))
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("transport", &self.transport.as_ref().map(|_| "custom"))
            .finish()
    }
}

fn check_timeout(timeout: Duration) -> Result<()> {
    if timeout.is_zero() {
        return Err(ConfigError::new("The timeout must be greater than zero.").into());
    }
    Ok(())
}

fn check_base_url(value: &str) -> Result<String> {
    let invalid = || ConfigError::new("`base_url` must be an absolute http(s) URL.");
    let url = reqwest::Url::parse(value).map_err(|_| invalid())?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(invalid().into());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ConfigError::new(
            "`base_url` must not contain credentials, a query string or a fragment.",
        )
        .into());
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_urls_are_checked_and_normalised() {
        assert_eq!(
            check_base_url("https://api.lettermint.co/v1/").unwrap(),
            "https://api.lettermint.co/v1"
        );
        assert_eq!(
            check_base_url("http://127.0.0.1:8080/v1").unwrap(),
            "http://127.0.0.1:8080/v1"
        );
        for bad in [
            "",
            "api.lettermint.co",
            "ftp://example.com",
            "https://user:pass@example.com",
            "https://example.com/v1?x=1",
            "https://example.com/v1#top",
        ] {
            assert!(check_base_url(bad).is_err(), "{bad}");
        }
    }
}
