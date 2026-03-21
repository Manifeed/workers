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

    pub fn is_equivalent_for_reporting(&self, other: &Self) -> bool {
        self.active == other.active
            && self.connection_state == other.connection_state
            && self.pending_tasks == other.pending_tasks
            && self.current_task_id == other.current_task_id
            && self.current_execution_id == other.current_execution_id
            && self.last_error == other.last_error
            && self.desired_state == other.desired_state
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

#[cfg(test)]
mod tests {
    use super::RssGatewayState;

    #[test]
    fn reporting_equivalence_ignores_volatile_feed_progress_fields() {
        let base = RssGatewayState {
            active: true,
            connection_state: "processing".to_string(),
            pending_tasks: 2,
            current_task_id: Some(21),
            current_execution_id: Some(21),
            current_task_label: Some("job a [1 / 10 feeds]".to_string()),
            current_feed_id: Some(11),
            current_feed_url: Some("https://example.com/feed-a.xml".to_string()),
            last_error: None,
            desired_state: Some("running".to_string()),
        };
        let changed_progress = RssGatewayState {
            current_task_label: Some("job a [9 / 10 feeds]".to_string()),
            current_feed_id: Some(19),
            current_feed_url: Some("https://example.com/feed-b.xml".to_string()),
            ..base.clone()
        };

        assert!(base.is_equivalent_for_reporting(&changed_progress));
    }
}
