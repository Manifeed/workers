use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use manifeed_worker_common::{
    ApiClient, CurrentTaskSnapshot, WorkerAuthenticator, WorkerError, WorkerStatusHandle,
};
use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;
use tracing::warn;

use crate::config::RssWorkerConfig;
use crate::error::{Result, RssWorkerError};
use crate::gateway::{
    build_rss_task_result_payload, derive_hmac_secret, new_nonce, sign_payload, utc_timestamp_now,
    WorkerLeaseRead, WorkerSessionOpenRead, WorkerSessionOpenRequest, WorkerTaskClaimRequest,
    WorkerTaskCompleteRequest, WorkerTaskFailRequest,
};
use crate::model::{ClaimedRssTask, RawFeedScrapeResult, RssFeedPayload};
use crate::worker::{RssGateway, RssGatewayState};

const NETWORK_RETRY_DELAY_SECONDS: u64 = 5;
const WORKER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
struct RssTaskPayload {
    job_id: String,
    requested_at: chrono::DateTime<chrono::Utc>,
    ingest: bool,
    feeds: Vec<RssFeedPayload>,
}

#[derive(Clone)]
struct V2LeaseMetadata {
    session_id: String,
    lease_id: String,
    trace_id: String,
    task_type: String,
    worker_version: Option<String>,
}

#[derive(Clone)]
struct V2SessionState {
    session_id: String,
    expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct HttpRssGateway {
    api_client: ApiClient,
    authenticator: WorkerAuthenticator,
    lease_seconds: u32,
    session_ttl_seconds: u32,
    task_type: String,
    worker_version: String,
    hmac_secret: String,
    session: Arc<Mutex<Option<V2SessionState>>>,
    lease_metadata: Arc<Mutex<HashMap<(u64, u64), V2LeaseMetadata>>>,
    status: WorkerStatusHandle,
}

impl HttpRssGateway {
    pub fn new(config: &RssWorkerConfig, status: WorkerStatusHandle) -> Result<Self> {
        Ok(Self {
            api_client: ApiClient::new(config.api_url.clone())?
                .with_traffic_observer(std::sync::Arc::new(status.clone())),
            authenticator: WorkerAuthenticator::new(config.auth.clone())?,
            lease_seconds: config.lease_seconds,
            session_ttl_seconds: config.session_ttl_seconds,
            task_type: "rss.fetch".to_string(),
            worker_version: WORKER_VERSION.to_string(),
            hmac_secret: derive_hmac_secret(config.auth.api_key.as_str()),
            session: Arc::new(Mutex::new(None)),
            lease_metadata: Arc::new(Mutex::new(HashMap::new())),
            status,
        })
    }

    fn bearer_token(&self) -> &str {
        self.authenticator.bearer_token()
    }

    fn parse_v2_claim(lease: WorkerLeaseRead) -> Result<(ClaimedRssTask, V2LeaseMetadata)> {
        let payload = serde_json::from_value::<RssTaskPayload>(lease.payload.clone())
            .map_err(|error| RssWorkerError::InvalidPayload(error.to_string()))?;
        Ok((
            ClaimedRssTask {
                task_id: lease.task_id,
                execution_id: lease.execution_id,
                job_id: payload.job_id,
                requested_at: payload.requested_at,
                ingest: payload.ingest,
                feeds: payload.feeds,
            },
            V2LeaseMetadata {
                session_id: String::new(),
                lease_id: lease.lease_id,
                trace_id: lease.trace_id,
                task_type: lease.task_type,
                worker_version: lease.worker_version,
            },
        ))
    }

    async fn claim_once(&self, count: usize) -> Result<Vec<ClaimedRssTask>> {
        self.claim_once_v2(count).await
    }

    async fn claim_once_v2(&self, count: usize) -> Result<Vec<ClaimedRssTask>> {
        let session = self.ensure_v2_session().await?;
        let leases = self
            .api_client
            .post_json::<_, Vec<WorkerLeaseRead>>(
                "/workers/tasks/claim",
                &WorkerTaskClaimRequest {
                    session_id: session.session_id.clone(),
                    task_type: self.task_type.clone(),
                    worker_version: Some(self.worker_version.clone()),
                    count: count.min(u32::MAX as usize) as u32,
                    lease_seconds: self.lease_seconds,
                },
                Some(self.bearer_token()),
            )
            .await?;

        let mut claimed_tasks = Vec::new();
        let mut lease_metadata = self.lock_lease_metadata()?;
        for lease in leases {
            let (task, mut metadata) = Self::parse_v2_claim(lease)?;
            metadata.session_id = session.session_id.clone();
            lease_metadata.insert((task.task_id, task.execution_id), metadata);
            claimed_tasks.push(task);
        }
        let _ = self.status.mark_server_connected();
        Ok(claimed_tasks)
    }

    async fn complete_once(
        &self,
        task_id: u64,
        execution_id: u64,
        results: &[RawFeedScrapeResult],
    ) -> Result<()> {
        self.complete_once_v2(task_id, execution_id, results).await
    }

    async fn complete_once_v2(
        &self,
        task_id: u64,
        execution_id: u64,
        results: &[RawFeedScrapeResult],
    ) -> Result<()> {
        let metadata = self
            .lease_metadata_for(task_id, execution_id)?
            .ok_or_else(|| {
                RssWorkerError::Runtime(format!(
                    "missing lease metadata for task {task_id}:{execution_id}"
                ))
            })?;
        let result_payload = build_rss_task_result_payload(results);
        let signed_at = utc_timestamp_now();
        let nonce = new_nonce();
        let signature = sign_payload(
            &self.hmac_secret,
            &json!({
                "lease_id": metadata.lease_id,
                "nonce": nonce,
                "result_payload": result_payload,
                "session_id": metadata.session_id,
                "signed_at": signed_at,
                "task_type": metadata.task_type,
                "trace_id": metadata.trace_id,
                "worker_version": metadata.worker_version,
            }),
        )?;
        self.api_client
            .post_json::<_, serde_json::Value>(
                "/workers/tasks/complete",
                &WorkerTaskCompleteRequest {
                    session_id: metadata.session_id.clone(),
                    lease_id: metadata.lease_id.clone(),
                    trace_id: metadata.trace_id.clone(),
                    task_type: metadata.task_type.clone(),
                    worker_version: metadata.worker_version.clone(),
                    signed_at,
                    nonce,
                    signature,
                    result_payload,
                },
                Some(self.bearer_token()),
            )
            .await?;
        self.remove_lease_metadata(task_id, execution_id)?;
        let _ = self.status.mark_server_connected();
        let _ = self.status.record_completed_task();
        Ok(())
    }

    async fn fail_once(&self, task_id: u64, execution_id: u64, error_message: &str) -> Result<()> {
        self.fail_once_v2(task_id, execution_id, error_message)
            .await
    }

    async fn fail_once_v2(
        &self,
        task_id: u64,
        execution_id: u64,
        error_message: &str,
    ) -> Result<()> {
        let metadata = self
            .lease_metadata_for(task_id, execution_id)?
            .ok_or_else(|| {
                RssWorkerError::Runtime(format!(
                    "missing lease metadata for failed task {task_id}:{execution_id}"
                ))
            })?;
        let signed_at = utc_timestamp_now();
        let nonce = new_nonce();
        let signature = sign_payload(
            &self.hmac_secret,
            &json!({
                "error_message": error_message,
                "lease_id": metadata.lease_id,
                "nonce": nonce,
                "session_id": metadata.session_id,
                "signed_at": signed_at,
                "task_type": metadata.task_type,
                "trace_id": metadata.trace_id,
                "worker_version": metadata.worker_version,
            }),
        )?;
        self.api_client
            .post_json::<_, serde_json::Value>(
                "/workers/tasks/fail",
                &WorkerTaskFailRequest {
                    session_id: metadata.session_id.clone(),
                    lease_id: metadata.lease_id.clone(),
                    trace_id: metadata.trace_id.clone(),
                    task_type: metadata.task_type.clone(),
                    worker_version: metadata.worker_version.clone(),
                    signed_at,
                    nonce,
                    signature,
                    error_message: error_message.to_string(),
                },
                Some(self.bearer_token()),
            )
            .await?;
        self.remove_lease_metadata(task_id, execution_id)?;
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

    async fn ensure_v2_session(&self) -> Result<V2SessionState> {
        if let Some(session) = self.current_session()? {
            if session.expires_at > Utc::now() + ChronoDuration::seconds(60) {
                return Ok(session);
            }
        }

        let session = self
            .api_client
            .post_json::<_, WorkerSessionOpenRead>(
                "/workers/sessions/open",
                &WorkerSessionOpenRequest {
                    task_type: self.task_type.clone(),
                    worker_version: Some(self.worker_version.clone()),
                    session_ttl_seconds: self.session_ttl_seconds,
                },
                Some(self.bearer_token()),
            )
            .await?;
        let next_session = V2SessionState {
            session_id: session.session_id,
            expires_at: session.expires_at,
        };
        self.store_session(next_session.clone())?;
        let _ = self.status.mark_server_connected();
        Ok(next_session)
    }

    fn current_session(&self) -> Result<Option<V2SessionState>> {
        self.session
            .lock()
            .map(|session| session.clone())
            .map_err(|_| RssWorkerError::Runtime("session mutex poisoned".to_string()))
    }

    fn store_session(&self, session: V2SessionState) -> Result<()> {
        self.session
            .lock()
            .map_err(|_| RssWorkerError::Runtime("session mutex poisoned".to_string()))?
            .replace(session);
        Ok(())
    }

    fn lease_metadata_for(
        &self,
        task_id: u64,
        execution_id: u64,
    ) -> Result<Option<V2LeaseMetadata>> {
        Ok(self
            .lock_lease_metadata()?
            .get(&(task_id, execution_id))
            .cloned())
    }

    fn remove_lease_metadata(&self, task_id: u64, execution_id: u64) -> Result<()> {
        self.lock_lease_metadata()?.remove(&(task_id, execution_id));
        Ok(())
    }

    fn lock_lease_metadata(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<(u64, u64), V2LeaseMetadata>>> {
        self.lease_metadata
            .lock()
            .map_err(|_| RssWorkerError::Runtime("lease metadata mutex poisoned".to_string()))
    }
}

#[async_trait]
impl RssGateway for HttpRssGateway {
    async fn claim(&self, count: usize) -> Result<Vec<ClaimedRssTask>> {
        loop {
            match self.claim_once(count).await {
                Ok(tasks) => return Ok(tasks),
                Err(error) if Self::should_retry(&error) => {
                    let _ = self
                        .status
                        .mark_server_disconnected(error.user_facing_message());
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
                    let _ = self
                        .status
                        .mark_server_disconnected(error.user_facing_message());
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
                    let _ = self
                        .status
                        .mark_server_disconnected(error.user_facing_message());
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
        Ok(())
    }
}
