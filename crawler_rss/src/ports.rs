use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::model::{ClaimedRssTask, RawFeedScrapeResult, RssFeedPayload};
use crate::worker::RssGatewayState;

#[async_trait]
pub trait TaskGateway {
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

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub use TaskGateway as RssGateway;
