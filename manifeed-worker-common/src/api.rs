use std::sync::Arc;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Result, WorkerError};

pub trait ApiTrafficObserver: Send + Sync {
    fn record_transfer(&self, request_bytes: usize, response_bytes: usize);
}

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
    traffic_observer: Option<Arc<dyn ApiTrafficObserver>>,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(WorkerError::Config("MANIFEED_API_URL is empty".to_string()));
        }
        let client = reqwest::Client::builder()
            .user_agent(format!(
                "manifeed-worker-rust/{}",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        Ok(Self {
            base_url,
            client,
            traffic_observer: None,
        })
    }

    pub fn with_traffic_observer(mut self, traffic_observer: Arc<dyn ApiTrafficObserver>) -> Self {
        self.traffic_observer = Some(traffic_observer);
        self
    }

    pub async fn get_json<T>(&self, path: &str, bearer_token: Option<&str>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request = self.authorized_request(self.client.get(self.url(path)?), bearer_token);
        let (payload, response_bytes) = self.handle_response(request.send().await?).await?;
        self.record_transfer(0, response_bytes);
        Ok(payload)
    }

    pub async fn post_json<TReq, TRes>(
        &self,
        path: &str,
        payload: &TReq,
        bearer_token: Option<&str>,
    ) -> Result<TRes>
    where
        TReq: Serialize + ?Sized,
        TRes: DeserializeOwned,
    {
        let request_body = serde_json::to_vec(payload)?;
        let request = self.authorized_request(self.client.post(self.url(path)?), bearer_token);
        let (payload, response_bytes) = self
            .handle_response(
                request
                    .header(CONTENT_TYPE, "application/json")
                    .body(request_body.clone())
                    .send()
                    .await?,
            )
            .await?;
        self.record_transfer(request_body.len(), response_bytes);
        Ok(payload)
    }

    pub async fn post_json_bytes<TRes>(
        &self,
        path: &str,
        request_body: Vec<u8>,
        bearer_token: Option<&str>,
    ) -> Result<TRes>
    where
        TRes: DeserializeOwned,
    {
        let request = self.authorized_request(self.client.post(self.url(path)?), bearer_token);
        let (payload, response_bytes) = self
            .handle_response(
                request
                    .header(CONTENT_TYPE, "application/json")
                    .body(request_body.clone())
                    .send()
                    .await?,
            )
            .await?;
        self.record_transfer(request_body.len(), response_bytes);
        Ok(payload)
    }

    fn url(&self, path: &str) -> Result<String> {
        let normalized_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        Ok(format!("{}{}", self.base_url, normalized_path))
    }

    fn authorized_request(
        &self,
        request: reqwest::RequestBuilder,
        bearer_token: Option<&str>,
    ) -> reqwest::RequestBuilder {
        if let Some(token) = bearer_token {
            request.header(AUTHORIZATION, format!("Bearer {token}"))
        } else {
            request
        }
    }

    fn record_transfer(&self, request_bytes: usize, response_bytes: usize) {
        if let Some(observer) = &self.traffic_observer {
            observer.record_transfer(request_bytes, response_bytes);
        }
    }

    async fn handle_response<T>(&self, response: reqwest::Response) -> Result<(T, usize)>
    where
        T: DeserializeOwned,
    {
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            let message = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| {
                    value
                        .get("message")
                        .and_then(|detail| detail.as_str())
                        .map(str::to_string)
                        .or_else(|| {
                            value
                                .get("detail")
                                .and_then(|detail| detail.as_str())
                                .map(str::to_string)
                        })
                })
                .unwrap_or_else(|| String::from_utf8_lossy(&bytes).to_string());
            return Err(WorkerError::Api {
                status: status.as_u16(),
                message,
            });
        }
        let payload = serde_json::from_slice::<T>(&bytes).map_err(|error| {
            let preview = response_body_preview(&bytes);
            WorkerError::ResponseDecode(format!(
                "expected {} but received invalid JSON payload: {error}; body={preview}",
                std::any::type_name::<T>()
            ))
        })?;
        Ok((payload, bytes.len()))
    }
}

fn response_body_preview(bytes: &[u8]) -> String {
    const MAX_PREVIEW_CHARS: usize = 400;

    let body = String::from_utf8_lossy(bytes);
    let preview = body.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
    if body.chars().count() > MAX_PREVIEW_CHARS {
        format!("{preview}...")
    } else {
        preview
    }
}
