use std::fmt::{Display, Formatter};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH};
use reqwest::{StatusCode, Url};
use tokio::sync::Mutex;

use super::normalize::normalize_sources;
use crate::error::{Result, RssWorkerError};
use crate::logging::stdout_log;
use crate::model::{RawFeedScrapeResult, RssFeedPayload};
use crate::worker::FeedFetcher;

pub(super) const SIMPLE_FETCHPROTECTION: u8 = 1;
pub(super) const ADVANCED_FETCHPROTECTION: u8 = 2;
const DEFAULT_RSS_HEADERS: [(&str, &str); 4] = [
    ("User-Agent", "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0"),
    ("Accept-Language", "fr-FR,fr;q=0.9,en-US;q=0.7,en;q=0.6"),
    (
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,application/rss+xml,application/atom+xml;q=0.8,*/*;q=0.7",
    ),
    ("Accept-Encoding", "gzip, deflate, br"),
];

pub struct HttpFeedFetcher {
    client: reqwest::Client,
    host_next_request_at: Arc<Mutex<std::collections::HashMap<String, tokio::time::Instant>>>,
    host_max_requests_per_second: u32,
    fetch_retry_count: u32,
}

#[derive(Debug)]
struct FetchAttemptError {
    kind: FetchAttemptKind,
    status_code: Option<u16>,
    message: String,
}

#[derive(Debug, Eq, PartialEq)]
enum FetchAttemptKind {
    Permanent,
    Transient,
}

impl Display for FetchAttemptError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FetchAttemptError {}

impl FetchAttemptError {
    fn permanent(status_code: Option<u16>, message: impl Into<String>) -> Self {
        Self {
            kind: FetchAttemptKind::Permanent,
            status_code,
            message: message.into(),
        }
    }

    fn transient(status_code: Option<u16>, message: impl Into<String>) -> Self {
        Self {
            kind: FetchAttemptKind::Transient,
            status_code,
            message: message.into(),
        }
    }

    fn is_transient(&self) -> bool {
        self.kind == FetchAttemptKind::Transient
    }
}

impl HttpFeedFetcher {
    pub fn new(
        host_max_requests_per_second: u32,
        request_timeout_seconds: u64,
        fetch_retry_count: u32,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(request_timeout_seconds.max(1)))
            .build()?;
        Ok(Self {
            client,
            host_next_request_at: Arc::new(Mutex::new(std::collections::HashMap::new())),
            host_max_requests_per_second,
            fetch_retry_count,
        })
    }

    async fn fetch_once(
        &self,
        job_id: &str,
        ingest: bool,
        feed: &RssFeedPayload,
        resolved_fetchprotection: u8,
    ) -> std::result::Result<RawFeedScrapeResult, FetchAttemptError> {
        if feed.fetchprotection == 0 {
            return Ok(RawFeedScrapeResult::error(
                job_id,
                ingest,
                feed,
                None,
                Some(0),
                "Blocked by fetch protection",
            ));
        }

        self.wait_for_rate_limit(feed).await;

        let response = self
            .client
            .get(&feed.feed_url)
            .headers(
                build_headers(feed, resolved_fetchprotection)
                    .map_err(|error| FetchAttemptError::permanent(None, error.to_string()))?,
            )
            .send()
            .await
            .map_err(classify_reqwest_error)?;
        let response_status = response.status();

        let response_etag = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let response_last_modified = response
            .headers()
            .get("last-modified")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| httpdate::parse_http_date(value).ok())
            .map(DateTime::<Utc>::from);
        stdout_log(format!(
            "fetch {} {} {} {}",
            feed.feed_id,
            response_status.as_u16(),
            if response_status.is_success() {
                "ok"
            } else {
                "ko"
            },
            feed.feed_url
        ));

        if response_status == StatusCode::NOT_MODIFIED {
            return Ok(RawFeedScrapeResult::not_modified(
                job_id,
                ingest,
                feed,
                response_status.as_u16(),
                response_etag,
                response_last_modified,
                resolved_fetchprotection,
            ));
        }

        if is_transient_status(response_status) {
            return Err(FetchAttemptError::transient(
                Some(response_status.as_u16()),
                format!(
                    "Transient HTTP status {} for {}",
                    response_status, feed.feed_url
                ),
            ));
        }

        if !response_status.is_success() {
            return Err(FetchAttemptError::permanent(
                Some(response_status.as_u16()),
                format!("HTTP status {} for {}", response_status, feed.feed_url),
            ));
        }

        let bytes = response.bytes().await.map_err(classify_reqwest_error)?;
        let parsed_feed = feed_rs::parser::parse(Cursor::new(bytes)).map_err(|error| {
            FetchAttemptError::permanent(
                Some(response_status.as_u16()),
                format!("Failed to parse RSS feed {}: {}", feed.feed_url, error),
            )
        })?;
        stdout_log(format!("parsing {} ok {}", feed.feed_id, feed.feed_url));
        let normalized_sources = normalize_sources(&parsed_feed, feed.last_db_article_published_at);

        Ok(RawFeedScrapeResult::success(
            job_id,
            ingest,
            feed,
            response_status.as_u16(),
            response_etag,
            response_last_modified.or(parsed_feed.updated),
            resolved_fetchprotection,
            normalized_sources,
        ))
    }

    async fn wait_for_rate_limit(&self, feed: &RssFeedPayload) {
        let key = scheduling_host_key(feed);
        let interval = tokio::time::Duration::from_millis(
            1000_u64 / self.host_max_requests_per_second.max(1) as u64,
        );
        let reserved_at = {
            let mut next_request_at = self.host_next_request_at.lock().await;
            reserve_request_slot(
                &mut next_request_at,
                &key,
                tokio::time::Instant::now(),
                interval,
            )
        };
        let now = tokio::time::Instant::now();
        if reserved_at > now {
            tokio::time::sleep_until(reserved_at).await;
        }
    }
}

fn reserve_request_slot(
    next_request_at: &mut std::collections::HashMap<String, tokio::time::Instant>,
    key: &str,
    now: tokio::time::Instant,
    interval: tokio::time::Duration,
) -> tokio::time::Instant {
    let reserved_at = next_request_at.get(key).copied().unwrap_or(now).max(now);
    next_request_at.insert(key.to_string(), reserved_at + interval);
    reserved_at
}

#[async_trait]
impl FeedFetcher for HttpFeedFetcher {
    async fn fetch(
        &self,
        job_id: &str,
        ingest: bool,
        feed: &RssFeedPayload,
    ) -> Result<RawFeedScrapeResult> {
        let fetch_attempts = build_fetch_attempts(feed.fetchprotection, ingest);
        let mut last_result = None;

        for resolved_fetchprotection in fetch_attempts {
            let mut attempt_no = 0;
            loop {
                match self
                    .fetch_once(job_id, ingest, feed, resolved_fetchprotection)
                    .await
                {
                    Ok(result) => return Ok(result),
                    Err(error) if error.is_transient() && attempt_no < self.fetch_retry_count => {
                        attempt_no += 1;
                        tokio::time::sleep(Duration::from_millis(250 * u64::from(attempt_no)))
                            .await;
                    }
                    Err(error) => {
                        last_result = Some(RawFeedScrapeResult::error(
                            job_id,
                            ingest,
                            feed,
                            error.status_code,
                            Some(resolved_fetchprotection),
                            error.message,
                        ));
                        break;
                    }
                }
            }
        }

        Ok(last_result.unwrap_or_else(|| {
            RawFeedScrapeResult::error(
                job_id,
                ingest,
                feed,
                None,
                Some(feed.fetchprotection),
                "No fetch strategy available",
            )
        }))
    }
}

pub(crate) fn resolve_effective_host(feed: &RssFeedPayload) -> Option<String> {
    normalize_host(feed.host_header.as_deref()).or_else(|| {
        Url::parse(&feed.feed_url)
            .ok()
            .and_then(|url| url.host_str().map(|host| host.to_lowercase()))
    })
}

pub(crate) fn scheduling_host_key(feed: &RssFeedPayload) -> String {
    resolve_effective_host(feed).unwrap_or_else(|| format!("feed:{}", feed.feed_id))
}

pub(super) fn build_headers(
    feed: &RssFeedPayload,
    resolved_fetchprotection: u8,
) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for (key, value) in DEFAULT_RSS_HEADERS {
        headers.insert(
            key,
            HeaderValue::from_str(value)
                .map_err(|error| RssWorkerError::InvalidHeader(error.to_string()))?,
        );
    }
    if resolved_fetchprotection == ADVANCED_FETCHPROTECTION {
        if let Some(host) = &feed.host_header {
            headers.insert(
                "Host",
                HeaderValue::from_str(host)
                    .map_err(|error| RssWorkerError::InvalidHeader(error.to_string()))?,
            );
            headers.insert(
                "Origin",
                HeaderValue::from_str(&format!("https://{host}"))
                    .map_err(|error| RssWorkerError::InvalidHeader(error.to_string()))?,
            );
            headers.insert(
                "Referer",
                HeaderValue::from_str(&format!("https://{host}/"))
                    .map_err(|error| RssWorkerError::InvalidHeader(error.to_string()))?,
            );
        }
    }
    if let Some(etag) = &feed.etag {
        headers.insert(
            IF_NONE_MATCH,
            HeaderValue::from_str(etag)
                .map_err(|error| RssWorkerError::InvalidHeader(error.to_string()))?,
        );
    }
    if let Some(last_update) = feed.last_update {
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(last_update.into()))
                .map_err(|error| RssWorkerError::InvalidHeader(error.to_string()))?,
        );
    }
    Ok(headers)
}

pub(super) fn build_fetch_attempts(fetchprotection: u8, ingest: bool) -> Vec<u8> {
    if fetchprotection == 0 {
        return vec![0];
    }

    if ingest {
        return match fetchprotection {
            SIMPLE_FETCHPROTECTION => vec![SIMPLE_FETCHPROTECTION, ADVANCED_FETCHPROTECTION],
            ADVANCED_FETCHPROTECTION => vec![ADVANCED_FETCHPROTECTION, SIMPLE_FETCHPROTECTION],
            _ => vec![fetchprotection],
        };
    }

    vec![SIMPLE_FETCHPROTECTION, ADVANCED_FETCHPROTECTION]
}

fn normalize_host(raw_host: Option<&str>) -> Option<String> {
    let value = raw_host?.trim();
    if value.is_empty() {
        return None;
    }

    let with_scheme = if value.contains("://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };

    Url::parse(&with_scheme)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_lowercase()))
}

fn classify_reqwest_error(error: reqwest::Error) -> FetchAttemptError {
    if let Some(status) = error.status() {
        if is_transient_status(status) {
            return FetchAttemptError::transient(
                Some(status.as_u16()),
                format!("Transient HTTP status {status}: {error}"),
            );
        }
        return FetchAttemptError::permanent(
            Some(status.as_u16()),
            format!("HTTP status {status}: {error}"),
        );
    }

    if error.is_timeout() || error.is_connect() || error.is_body() || error.is_request() {
        return FetchAttemptError::transient(None, error.to_string());
    }

    FetchAttemptError::permanent(None, error.to_string())
}

fn is_transient_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}
