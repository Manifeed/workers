use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use manifeed_worker_common::{
    ApiClient, CurrentTaskSnapshot, WorkerAuthenticator, WorkerError, WorkerStatusHandle,
    WorkerTaskClaim, WorkerTaskClaimRequest,
};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::warn;

use crate::config::RssWorkerConfig;
use crate::error::{Result, RssWorkerError};
use crate::model::{ClaimedRssTask, RawFeedScrapeResult, RssFeedPayload};
use crate::worker::{RssGateway, RssGatewayState};

const NETWORK_RETRY_DELAY_SECONDS: u64 = 5;

#[derive(Deserialize)]
struct RssTaskPayload {
    job_id: String,
    requested_at: chrono::DateTime<chrono::Utc>,
    ingest: bool,
    feeds: Vec<RssFeedPayload>,
}

#[derive(Serialize)]
struct RssTaskCompleteRequest {
    task_id: u64,
    execution_id: u64,
    result_events: Vec<RssResultEvent>,
}

#[derive(Serialize)]
struct RssResultEvent {
    payload: RawFeedScrapeResult,
}

#[derive(Serialize)]
struct RssTaskFailRequest {
    task_id: u64,
    execution_id: u64,
    error_message: String,
}

#[derive(Clone)]
pub struct HttpRssGateway {
    api_client: ApiClient,
    authenticator: WorkerAuthenticator,
    lease_seconds: u32,
    status: WorkerStatusHandle,
}

impl HttpRssGateway {
    pub fn new(config: &RssWorkerConfig, status: WorkerStatusHandle) -> Result<Self> {
        Ok(Self {
            api_client: ApiClient::new(config.api_url.clone())?
                .with_traffic_observer(std::sync::Arc::new(status.clone())),
            authenticator: WorkerAuthenticator::new(config.auth.clone())?,
            lease_seconds: config.lease_seconds,
            status,
        })
    }

    fn bearer_token(&self) -> &str {
        self.authenticator.bearer_token()
    }

    fn parse_claim(task: WorkerTaskClaim) -> Result<ClaimedRssTask> {
        let payload = serde_json::from_value::<RssTaskPayload>(task.payload)
            .map_err(|error| RssWorkerError::InvalidPayload(error.to_string()))?;
        Ok(ClaimedRssTask {
            task_id: task.task_id,
            execution_id: task.execution_id,
            job_id: payload.job_id,
            requested_at: payload.requested_at,
            ingest: payload.ingest,
            feeds: payload.feeds,
        })
    }

    async fn claim_once(&self, count: usize) -> Result<Vec<ClaimedRssTask>> {
        let tasks = self
            .api_client
            .post_json::<_, Vec<WorkerTaskClaim>>(
                "/workers/rss/claim",
                &WorkerTaskClaimRequest {
                    count: count.min(u32::MAX as usize) as u32,
                    lease_seconds: self.lease_seconds,
                    worker_version: None,
                },
                Some(self.bearer_token()),
            )
            .await?;
        let _ = self.status.mark_server_connected();
        tasks.into_iter().map(Self::parse_claim).collect()
    }

    async fn complete_once(
        &self,
        task_id: u64,
        execution_id: u64,
        results: &[RawFeedScrapeResult],
    ) -> Result<()> {
        self.api_client
            .post_json::<_, serde_json::Value>(
                "/workers/rss/complete",
                &RssTaskCompleteRequest {
                    task_id,
                    execution_id,
                    result_events: results
                        .iter()
                        .cloned()
                        .map(|payload| RssResultEvent { payload })
                        .collect(),
                },
                Some(self.bearer_token()),
            )
            .await?;
        let _ = self.status.mark_server_connected();
        let _ = self.status.mark_completed_task();
        Ok(())
    }

    async fn fail_once(&self, task_id: u64, execution_id: u64, error_message: &str) -> Result<()> {
        self.api_client
            .post_json::<_, serde_json::Value>(
                "/workers/rss/fail",
                &RssTaskFailRequest {
                    task_id,
                    execution_id,
                    error_message: error_message.to_string(),
                },
                Some(self.bearer_token()),
            )
            .await?;
        let _ = self.status.mark_server_connected();
        Ok(())
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
                        .unwrap_or_else(Utc::now);
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

    fn should_retry(error: &RssWorkerError) -> bool {
        matches!(
            error,
            RssWorkerError::Common(WorkerError::Http(_)) | RssWorkerError::Http(_)
        ) && error.is_network_error()
    }
}

#[async_trait]
impl RssGateway for HttpRssGateway {
    async fn claim(&self, count: usize) -> Result<Vec<ClaimedRssTask>> {
        loop {
            match self.claim_once(count).await {
                Ok(tasks) => return Ok(tasks),
                Err(error) if Self::should_retry(&error) => {
                    let _ = self.status.mark_server_disconnected(error.to_string());
                    warn!(
                        retry_delay_seconds = NETWORK_RETRY_DELAY_SECONDS,
                        "network error while claiming rss tasks, retrying: {error}"
                    );
                    sleep(Duration::from_secs(NETWORK_RETRY_DELAY_SECONDS)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn complete(
        &self,
        task_id: u64,
        execution_id: u64,
        results: Vec<RawFeedScrapeResult>,
    ) -> Result<()> {
        loop {
            match self.complete_once(task_id, execution_id, &results).await {
                Ok(()) => return Ok(()),
                Err(error) if Self::should_retry(&error) => {
                    let _ = self.status.mark_server_disconnected(error.to_string());
                    warn!(
                        task_id,
                        execution_id,
                        retry_delay_seconds = NETWORK_RETRY_DELAY_SECONDS,
                        "network error while completing rss task, retrying: {error}"
                    );
                    sleep(Duration::from_secs(NETWORK_RETRY_DELAY_SECONDS)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn fail(&self, task_id: u64, execution_id: u64, error_message: String) -> Result<()> {
        loop {
            match self.fail_once(task_id, execution_id, &error_message).await {
                Ok(()) => return Ok(()),
                Err(error) if Self::should_retry(&error) => {
                    let _ = self.status.mark_server_disconnected(error.to_string());
                    warn!(
                        task_id,
                        execution_id,
                        retry_delay_seconds = NETWORK_RETRY_DELAY_SECONDS,
                        "network error while failing rss task, retrying: {error}"
                    );
                    sleep(Duration::from_secs(NETWORK_RETRY_DELAY_SECONDS)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn update_state(&self, state: RssGatewayState) -> Result<()> {
        let state = state.sanitized();
        self.apply_gateway_state(&state);
        self.api_client
            .post_json::<_, serde_json::Value>(
                "/workers/rss/state",
                &state,
                Some(self.bearer_token()),
            )
            .await?;
        let _ = self.status.mark_server_connected();
        Ok(())
    }
}
