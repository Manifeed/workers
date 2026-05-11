use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Result, WorkerError};

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(WorkerError::Config("MANIFEED_API_URL is empty".to_string()).into());
        }
        Ok(Self {
            client: build_http_client(&base_url)?,
            base_url,
        })
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
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
        self.handle_response(
            request
                .header(CONTENT_TYPE, "application/json")
                .body(request_body)
                .send()
                .await?,
        )
        .await
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
        self.handle_response(
            request
                .header(CONTENT_TYPE, "application/json")
                .body(request_body)
                .send()
                .await?,
        )
        .await
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
        match bearer_token {
            Some(token) => request.header(AUTHORIZATION, format!("Bearer {token}")),
            None => request,
        }
    }

    async fn handle_response<T>(&self, response: reqwest::Response) -> Result<T>
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
            }
            .into());
        }
        serde_json::from_slice::<T>(&bytes).map_err(|error| {
            let preview = response_body_preview(&bytes);
            WorkerError::ResponseDecode(format!(
                "expected {} but received invalid JSON payload: {error}; body={preview}",
                std::any::type_name::<T>()
            ))
            .into()
        })
    }
}

fn build_http_client(base_url: &str) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder().user_agent(format!(
        "manifeed-crawler-rss/{}",
        env!("CARGO_PKG_VERSION")
    ));
    if should_accept_invalid_localhost_tls(base_url) {
        builder = builder.danger_accept_invalid_certs(true);
    }
    Ok(builder.build()?)
}

fn should_accept_invalid_localhost_tls(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host.eq_ignore_ascii_case("127.0.0.1")
        || host == "::1"
        || host == "[::1]"
        || host.to_ascii_lowercase().ends_with(".localhost")
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
