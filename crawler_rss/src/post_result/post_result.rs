use async_trait::async_trait;

use crate::api::ApiClient;
use crate::auth::WorkerAuthenticator;
use crate::claim::parse_claimed_rss_task;
use crate::config::RssWorkerConfig;
use crate::dedup::build_rss_task_result_payload;
use crate::error::Result;
use crate::gateway::WorkerGatewayClient;
use crate::model::{ClaimedRssTask, RawFeedScrapeResult};
use crate::protocol::{CanonicalJsonMode, WorkerLeaseRead};
use crate::worker::RssGateway;

const WORKER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct HttpRssGateway {
    client: WorkerGatewayClient,
}

impl HttpRssGateway {
    pub fn new(config: &RssWorkerConfig) -> Result<Self> {
        let api_client = ApiClient::new(config.api_url.clone())?;
        let authenticator = WorkerAuthenticator::new(config.auth.clone())?;
        Ok(Self {
            client: WorkerGatewayClient::new(
                api_client,
                authenticator,
                config.lease_seconds,
                config.session_ttl_seconds,
                "rss.fetch",
                WORKER_VERSION,
                config.auth.api_key.as_str(),
                CanonicalJsonMode::PreserveNumberFormatting,
            ),
        })
    }

    fn parse_claim(lease: WorkerLeaseRead) -> Result<(u64, u64, ClaimedRssTask)> {
        parse_claimed_rss_task(lease)
    }
}

#[async_trait]
impl RssGateway for HttpRssGateway {
    async fn claim(&self, count: usize) -> Result<Vec<ClaimedRssTask>> {
        self.client.claim_tasks(count, Self::parse_claim).await
    }

    async fn complete(
        &self,
        task_id: u64,
        execution_id: u64,
        results: Vec<RawFeedScrapeResult>,
    ) -> Result<()> {
        let payload = build_rss_task_result_payload(&results);
        self.client
            .complete_task(task_id, execution_id, &payload)
            .await?;
        Ok(())
    }

    async fn fail(&self, task_id: u64, execution_id: u64, error_message: String) -> Result<()> {
        self.client
            .fail_task(task_id, execution_id, &error_message)
            .await
    }

    async fn update_state(&self, _state: crate::worker::RssGatewayState) -> Result<()> {
        Ok(())
    }
}
