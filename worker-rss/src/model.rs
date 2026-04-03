use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RssFeedPayload {
    pub feed_id: u64,
    pub feed_url: String,
    pub company_id: Option<u64>,
    pub host_header: Option<String>,
    pub fetchprotection: u8,
    pub etag: Option<String>,
    pub last_update: Option<DateTime<Utc>>,
    pub last_db_article_published_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct ClaimedRssTask {
    pub task_id: u64,
    pub execution_id: u64,
    pub job_id: String,
    pub requested_at: DateTime<Utc>,
    pub ingest: bool,
    pub feeds: Vec<RssFeedPayload>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RssSource {
    pub title: String,
    pub url: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RssResultStatus {
    Success,
    NotModified,
    Error,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RawFeedScrapeResult {
    pub job_id: String,
    pub ingest: bool,
    pub feed_id: u64,
    pub feed_url: String,
    pub status: RssResultStatus,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
    pub new_etag: Option<String>,
    pub new_last_update: Option<DateTime<Utc>>,
    pub fetchprotection: u8,
    pub resolved_fetchprotection: Option<u8>,
    pub sources: Vec<RssSource>,
}

impl RawFeedScrapeResult {
    pub fn success(
        job_id: impl Into<String>,
        ingest: bool,
        feed: &RssFeedPayload,
        status_code: u16,
        new_etag: Option<String>,
        new_last_update: Option<DateTime<Utc>>,
        resolved_fetchprotection: u8,
        sources: Vec<RssSource>,
    ) -> Self {
        Self {
            status: RssResultStatus::Success,
            status_code: Some(status_code),
            new_etag,
            new_last_update,
            resolved_fetchprotection: Some(resolved_fetchprotection),
            sources,
            ..Self::new(job_id, ingest, feed)
        }
    }

    pub fn not_modified(
        job_id: impl Into<String>,
        ingest: bool,
        feed: &RssFeedPayload,
        status_code: u16,
        new_etag: Option<String>,
        new_last_update: Option<DateTime<Utc>>,
        resolved_fetchprotection: u8,
    ) -> Self {
        Self {
            status: RssResultStatus::NotModified,
            status_code: Some(status_code),
            new_etag,
            new_last_update,
            resolved_fetchprotection: Some(resolved_fetchprotection),
            ..Self::new(job_id, ingest, feed)
        }
    }

    pub fn error(
        job_id: impl Into<String>,
        ingest: bool,
        feed: &RssFeedPayload,
        status_code: Option<u16>,
        resolved_fetchprotection: Option<u8>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            status: RssResultStatus::Error,
            status_code,
            error_message: Some(error_message.into()),
            new_etag: feed.etag.clone(),
            new_last_update: feed.last_update,
            resolved_fetchprotection,
            ..Self::new(job_id, ingest, feed)
        }
    }

    fn new(job_id: impl Into<String>, ingest: bool, feed: &RssFeedPayload) -> Self {
        Self {
            job_id: job_id.into(),
            ingest,
            feed_id: feed.feed_id,
            feed_url: feed.feed_url.clone(),
            status: RssResultStatus::Error,
            status_code: None,
            error_message: None,
            new_etag: None,
            new_last_update: None,
            fetchprotection: feed.fetchprotection,
            resolved_fetchprotection: None,
            sources: Vec::new(),
        }
    }
}
