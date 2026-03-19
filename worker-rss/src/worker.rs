mod runtime;
mod scheduling;

use async_trait::async_trait;
use serde::Serialize;

use crate::error::Result;
use crate::model::{ClaimedRssTask, RawFeedScrapeResult, RssFeedPayload};

pub use runtime::RssWorker;

const MAX_CURRENT_TASK_LABEL_CHARS: usize = 255;
const MAX_CURRENT_FEED_URL_CHARS: usize = 1000;

#[async_trait]
pub trait RssGateway {
    async fn claim(&self, count: usize) -> Result<Vec<ClaimedRssTask>>;
    async fn complete(
        &self,
        task_id: u64,
        execution_id: u64,
        results: Vec<RawFeedScrapeResult>,
    ) -> Result<()>;
    async fn fail(&self, task_id: u64, execution_id: u64, error_message: String) -> Result<()>;
    async fn update_state(&self, state: RssGatewayState) -> Result<()>;
}

#[async_trait]
pub trait FeedFetcher {
    async fn fetch(
        &self,
        job_id: &str,
        ingest: bool,
        feed: &RssFeedPayload,
    ) -> Result<RawFeedScrapeResult>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RssGatewayState {
    pub active: bool,
    pub connection_state: String,
    pub pending_tasks: u32,
    pub current_task_id: Option<u64>,
    pub current_execution_id: Option<u64>,
    pub current_task_label: Option<String>,
    pub current_feed_id: Option<u64>,
    pub current_feed_url: Option<String>,
    pub last_error: Option<String>,
    pub desired_state: Option<String>,
}

impl RssGatewayState {
    pub fn sanitized(mut self) -> Self {
        self.current_task_label =
            truncate_optional_chars(self.current_task_label, MAX_CURRENT_TASK_LABEL_CHARS);
        self.current_feed_url =
            truncate_optional_chars(self.current_feed_url, MAX_CURRENT_FEED_URL_CHARS);
        self
    }
}

fn truncate_optional_chars(value: Option<String>, max_chars: usize) -> Option<String> {
    value.map(|value| truncate_chars(value, max_chars))
}

fn truncate_chars(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    value.chars().take(max_chars).collect()
}
