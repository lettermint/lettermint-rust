use crate::endpoints;
use crate::error::{Error, Result};
use async_trait::async_trait;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const DEFAULT_BASE_URL: &str = "https://api.lettermint.co/v1";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthMode {
    Sending,
    Api,
}

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub body: String,
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse>;
}

#[derive(Clone, Default)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(format!("Lettermint/{VERSION} (Rust)"))
            .build()?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
        let method =
            reqwest::Method::from_bytes(request.method.as_bytes()).expect("valid HTTP method");
        let mut headers = HeaderMap::new();
        for (key, value) in request.headers {
            let name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|_| Error::InvalidHeader(key.clone()))?;
            let value =
                HeaderValue::from_str(&value).map_err(|_| Error::InvalidHeader(key.clone()))?;
            headers.insert(name, value);
        }

        let mut builder = self.client.request(method, request.url).headers(headers);
        if let Some(body) = request.body {
            builder = builder.body(body);
        }

        let response = builder.send().await?;
        let status = response.status();
        let reason = status.canonical_reason().unwrap_or("").to_string();
        let body = response.text().await?;

        Ok(HttpResponse {
            status: status.as_u16(),
            reason,
            body,
        })
    }
}

#[derive(Clone)]
pub struct HttpClient {
    token: String,
    auth: AuthMode,
    base_url: String,
    transport: Arc<dyn Transport>,
}

impl HttpClient {
    pub fn new(token: impl Into<String>, auth: AuthMode) -> Result<Self> {
        Self::with_transport(token, auth, ReqwestTransport::new()?)
    }

    pub fn with_transport<T>(token: impl Into<String>, auth: AuthMode, transport: T) -> Result<Self>
    where
        T: Transport + 'static,
    {
        let token = token.into();
        if token.is_empty() {
            return Err(Error::MissingToken);
        }

        Ok(Self {
            token,
            auth,
            base_url: DEFAULT_BASE_URL.to_string(),
            transport: Arc::new(transport),
        })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    pub async fn get<T>(&self, path: &str, query: &[(&str, &str)]) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.request("GET", path, query, Option::<&()>::None, None)
            .await
    }

    pub async fn get_raw(&self, path: &str, query: &[(&str, &str)]) -> Result<String> {
        let response = self
            .send_request("GET", path, query, Option::<&()>::None, None)
            .await?;
        Ok(response.body)
    }

    pub async fn post<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + Sync + ?Sized,
    {
        self.request("POST", path, &[], Some(body), None).await
    }

    pub async fn post_with_headers<T, B>(
        &self,
        path: &str,
        body: &B,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + Sync + ?Sized,
    {
        self.request("POST", path, &[], Some(body), headers).await
    }

    pub async fn put<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + Sync + ?Sized,
    {
        self.request("PUT", path, &[], Some(body), None).await
    }

    pub async fn delete<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.request("DELETE", path, &[], Option::<&()>::None, None)
            .await
    }

    async fn request<T, B>(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&B>,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + Sync + ?Sized,
    {
        let response = self
            .send_request(method, path, query, body, headers)
            .await?;
        Ok(serde_json::from_str(&response.body)?)
    }

    async fn send_request<B>(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&B>,
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<HttpResponse>
    where
        B: Serialize + Sync + ?Sized,
    {
        let body = match body {
            Some(value) => Some(serde_json::to_string(value)?),
            None => None,
        };
        let request = HttpRequest {
            method: method.to_string(),
            url: self.url(path, query),
            headers: self.headers(headers),
            body,
        };
        let response = self.transport.send(request).await?;

        if response.status >= 400 {
            return Err(self.error_from_response(response));
        }

        Ok(response)
    }

    fn url(&self, path: &str, query: &[(&str, &str)]) -> String {
        let mut url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        if !query.is_empty() {
            let query = query
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}={}",
                        urlencoding::encode(key),
                        urlencoding::encode(value)
                    )
                })
                .collect::<Vec<_>>()
                .join("&");
            url.push('?');
            url.push_str(&query);
        }
        url
    }

    fn headers(&self, headers: Option<BTreeMap<String, String>>) -> BTreeMap<String, String> {
        let mut out = BTreeMap::from([
            (ACCEPT.as_str().to_string(), "application/json".to_string()),
            (
                CONTENT_TYPE.as_str().to_string(),
                "application/json".to_string(),
            ),
            (
                USER_AGENT.as_str().to_string(),
                format!("Lettermint/{VERSION} (Rust)"),
            ),
        ]);
        for (key, value) in headers.unwrap_or_default() {
            let lower = key.to_ascii_lowercase();
            if lower != "authorization" && lower != "x-lettermint-token" {
                out.insert(lower, value);
            }
        }
        match self.auth {
            AuthMode::Sending => {
                out.insert("x-lettermint-token".into(), self.token.clone());
            }
            AuthMode::Api => {
                out.insert("authorization".into(), format!("Bearer {}", self.token));
            }
        }
        out
    }

    fn error_from_response(&self, response: HttpResponse) -> Error {
        let body = serde_json::from_str::<serde_json::Value>(&response.body).ok();
        if response.status == 422 {
            let error_type = body
                .as_ref()
                .and_then(|value| value.get("error"))
                .and_then(|value| value.as_str())
                .unwrap_or("ValidationError")
                .to_string();
            return Error::Validation { error_type, body };
        }

        Error::Http {
            status: response.status,
            message: response.reason,
            body: redact_token(body, &self.token),
        }
    }
}

fn redact_token(body: Option<serde_json::Value>, token: &str) -> Option<serde_json::Value> {
    fn redact(value: serde_json::Value, token: &str) -> serde_json::Value {
        match value {
            serde_json::Value::String(value) => {
                serde_json::Value::String(value.replace(token, "[REDACTED]"))
            }
            serde_json::Value::Array(values) => serde_json::Value::Array(
                values
                    .into_iter()
                    .map(|value| redact(value, token))
                    .collect(),
            ),
            serde_json::Value::Object(values) => serde_json::Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, redact(value, token)))
                    .collect(),
            ),
            value => value,
        }
    }

    body.map(|value| redact(value, token))
}

#[derive(Clone)]
pub struct EmailClient {
    pub(crate) client: HttpClient,
}

impl EmailClient {
    pub async fn ping(&self) -> Result<String> {
        Ok(self.client.get_raw("/ping", &[]).await?.trim().to_string())
    }
}

#[derive(Clone)]
pub struct ApiClient {
    client: HttpClient,
}

impl ApiClient {
    pub async fn ping(&self) -> Result<String> {
        Ok(self.client.get_raw("/ping", &[]).await?.trim().to_string())
    }

    pub fn domains(&self) -> endpoints::Domains<'_> {
        endpoints::Domains::new(&self.client)
    }

    pub fn messages(&self) -> endpoints::Messages<'_> {
        endpoints::Messages::new(&self.client)
    }

    pub fn projects(&self) -> endpoints::Projects<'_> {
        endpoints::Projects::new(&self.client)
    }

    pub fn routes(&self) -> endpoints::Routes<'_> {
        endpoints::Routes::new(&self.client)
    }

    pub fn stats(&self) -> endpoints::Stats<'_> {
        endpoints::Stats::new(&self.client)
    }

    pub fn suppressions(&self) -> endpoints::Suppressions<'_> {
        endpoints::Suppressions::new(&self.client)
    }

    pub fn team(&self) -> endpoints::Team<'_> {
        endpoints::Team::new(&self.client)
    }

    pub fn webhooks(&self) -> endpoints::Webhooks<'_> {
        endpoints::Webhooks::new(&self.client)
    }
}

pub struct Lettermint;

impl Lettermint {
    pub fn email(token: impl Into<String>) -> Result<EmailClient> {
        Ok(EmailClient {
            client: HttpClient::new(token, AuthMode::Sending)?,
        })
    }

    pub fn api(token: impl Into<String>) -> Result<ApiClient> {
        Ok(ApiClient {
            client: HttpClient::new(token, AuthMode::Api)?,
        })
    }

    pub fn email_with_transport<T>(token: impl Into<String>, transport: T) -> Result<EmailClient>
    where
        T: Transport + 'static,
    {
        Ok(EmailClient {
            client: HttpClient::with_transport(token, AuthMode::Sending, transport)?,
        })
    }

    pub fn api_with_transport<T>(token: impl Into<String>, transport: T) -> Result<ApiClient>
    where
        T: Transport + 'static,
    {
        Ok(ApiClient {
            client: HttpClient::with_transport(token, AuthMode::Api, transport)?,
        })
    }
}
