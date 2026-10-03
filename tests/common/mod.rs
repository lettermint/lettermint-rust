//! A recording transport for the integration tests. It never touches the network.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use lettermint::{HttpRequest, HttpResponse, Lettermint, Result, Transport};

pub const SENDING_TOKEN: &str = "lm_SendingToken0123456789abcdefABCD";
pub const TEAM_TOKEN: &str = "lm_team_TeamToken0123456789abcdefABCDEFGHIJ";
pub const BASE_URL: &str = "https://api.example.test/v1";

/// One scripted reply.
#[derive(Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
    pub delay: Option<Duration>,
}

impl Reply {
    pub fn json(status: u16, body: serde_json::Value) -> Self {
        Self {
            status,
            headers: vec![("content-type", "application/json".into())],
            body: body.to_string().into_bytes(),
            delay: None,
        }
    }

    pub fn text(status: u16, content_type: &str, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type", content_type.into())],
            body: body.as_bytes().to_vec(),
            delay: None,
        }
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            delay: None,
        }
    }

    pub fn header(mut self, name: &'static str, value: &str) -> Self {
        self.headers.push((name, value.into()));
        self
    }

    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

/// A transport that records requests and answers with scripted replies (default: 202 pending).
#[derive(Clone, Default)]
pub struct MockTransport {
    requests: Arc<Mutex<Vec<HttpRequest>>>,
    replies: Arc<Mutex<VecDeque<Reply>>>,
    fallback: Arc<Mutex<Option<Reply>>>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reply(&self, reply: Reply) -> &Self {
        self.replies.lock().unwrap().push_back(reply);
        self
    }

    pub fn always(&self, reply: Reply) -> &Self {
        *self.fallback.lock().unwrap() = Some(reply);
        self
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn last(&self) -> HttpRequest {
        self.requests().pop().expect("a request was sent")
    }

    pub fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
        self.requests.lock().unwrap().push(request);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .or_else(|| self.fallback.lock().unwrap().clone())
            .unwrap_or_else(|| {
                Reply::json(
                    202,
                    serde_json::json!({"message_id": "msg_1", "status": "pending"}),
                )
            });
        if let Some(delay) = reply.delay {
            tokio::time::sleep(delay).await;
        }
        let mut headers = http::HeaderMap::new();
        for (name, value) in reply.headers {
            headers.append(name, value.parse().unwrap());
        }
        Ok(HttpResponse::new(reply.status, headers, reply.body))
    }
}

pub fn sending_client(transport: &MockTransport) -> Lettermint {
    Lettermint::builder()
        .sending_token(SENDING_TOKEN)
        .base_url(BASE_URL)
        .transport(transport.clone())
        .build()
        .unwrap()
}

pub fn team_client(transport: &MockTransport) -> Lettermint {
    Lettermint::builder()
        .team_token(TEAM_TOKEN)
        .base_url(BASE_URL)
        .transport(transport.clone())
        .build()
        .unwrap()
}

pub fn both_client(transport: &MockTransport) -> Lettermint {
    Lettermint::builder()
        .sending_token(SENDING_TOKEN)
        .team_token(TEAM_TOKEN)
        .base_url(BASE_URL)
        .transport(transport.clone())
        .build()
        .unwrap()
}

pub fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .map(|value| value.to_str().unwrap())
}

pub fn body_json(request: &HttpRequest) -> serde_json::Value {
    serde_json::from_slice(request.body.as_deref().expect("the request has a body")).unwrap()
}

/// The path and query after the base URL.
pub fn target(request: &HttpRequest) -> &str {
    request
        .url
        .strip_prefix(BASE_URL)
        .expect("the request goes to the base URL")
}
