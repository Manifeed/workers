use async_trait::async_trait;
use manifeed_worker_common::{
    ApiClient, CanonicalJsonMode, CurrentTaskSnapshot, WorkerAuthenticator, WorkerError,
    WorkerGatewayClient, WorkerLeaseRead, WorkerStatusHandle,
};
use serde::Deserialize;

use crate::config::RssWorkerConfig;
use crate::error::{Result, RssWorkerError};
use crate::gateway::build_rss_task_result_payload;
use crate::model::{ClaimedRssTask, RawFeedScrapeResult, RssFeedPayload};
use crate::worker::{RssGateway, RssGatewayState};

const WORKER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
struct RssTaskPayload {
    job_id: String,
    requested_at: chrono::DateTime<chrono::Utc>,
    ingest: bool,
    feeds: Vec<RssFeedPayload>,
}

#[derive(Clone)]
pub struct HttpRssGateway {
    client: WorkerGatewayClient,
    status: WorkerStatusHandle,
}

impl HttpRssGateway {
    pub fn new(config: &RssWorkerConfig, status: WorkerStatusHandle) -> Result<Self> {
        let api_client = ApiClient::new(config.api_url.clone())?
            .with_traffic_observer(std::sync::Arc::new(status.clone()));
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
                status.clone(),
            ),
            status,
        })
    }

    fn parse_claim(
        lease: WorkerLeaseRead,
    ) -> std::result::Result<(u64, u64, ClaimedRssTask), WorkerError> {
        let payload = serde_json::from_value::<RssTaskPayload>(lease.payload)
            .map_err(|error| WorkerError::ResponseDecode(error.to_string()))?;
        Ok((
            lease.task_id,
            lease.execution_id,
            ClaimedRssTask {
                task_id: lease.task_id,
                execution_id: lease.execution_id,
                job_id: payload.job_id,
                requested_at: payload.requested_at,
                ingest: payload.ingest,
                feeds: payload.feeds,
            },
        ))
    }

    fn apply_gateway_state(&self, state: &RssGatewayState) {
        let _ = self.status.update(|snapshot| {
            snapshot.phase = if state.active {
                manifeed_worker_common::WorkerPhase::Processing
            } else if state.last_error.is_some() {
                manifeed_worker_common::WorkerPhase::Error
            } else {
                manifeed_worker_common::WorkerPhase::Idle
            };
            snapshot.current_task = match (state.current_task_id, state.current_execution_id) {
                (Some(task_id), Some(execution_id)) => {
                    let started_at = snapshot
                        .current_task
                        .as_ref()
                        .filter(|task| task.task_id == task_id && task.execution_id == execution_id)
                        .map(|task| task.started_at)
                        .unwrap_or_else(chrono::Utc::now);
                    Some(CurrentTaskSnapshot {
                        task_id,
                        execution_id,
                        job_id: None,
                        label: state.current_task_label.clone(),
                        worker_version: None,
                        item_count: Some(state.pending_tasks as usize),
                        started_at,
                    })
                }
                _ => None,
            };
            snapshot.current_feed_id = state.current_feed_id;
            snapshot.current_feed_url = state.current_feed_url.clone();
            snapshot.last_error = state.last_error.clone();
        });
    }
}

#[async_trait]
impl RssGateway for HttpRssGateway {
    async fn claim(&self, count: usize) -> Result<Vec<ClaimedRssTask>> {
        self.client
            .claim_tasks(count, Self::parse_claim)
            .await
            .map_err(RssWorkerError::from)
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
            .await
            .map_err(RssWorkerError::from)?;
        let _ = self.status.record_completed_task();
        Ok(())
    }

    async fn fail(&self, task_id: u64, execution_id: u64, error_message: String) -> Result<()> {
        self.client
            .fail_task(task_id, execution_id, &error_message)
            .await
            .map_err(RssWorkerError::from)
    }

    async fn update_state(&self, state: RssGatewayState) -> Result<()> {
        self.apply_gateway_state(&state.sanitized());
        Ok(())
    }
}
