use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use manifeed_worker_common::{
    canonical_json, derive_hmac_secret, new_nonce, sign_payload, utc_timestamp_now, ApiClient,
    CanonicalJsonMode, WorkerAuthenticator, WorkerLeaseRead, WorkerSessionOpenRead,
    WorkerSessionOpenRequest, WorkerStatusHandle, WorkerTaskClaimRequest,
};
use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;
use tracing::warn;

use crate::config::EmbeddingWorkerConfig;
use crate::error::{EmbeddingWorkerError, Result};
use crate::gateway::build_embedding_task_result_payload;
use crate::worker::{
    ClaimedEmbeddingTask, EmbeddingGateway, EmbeddingResultSource, EmbeddingSourceInput,
};

const NETWORK_RETRY_DELAY_SECONDS: u64 = 5;

#[derive(Deserialize)]
struct EmbeddingTaskPayload {
    job_id: String,
    worker_version: String,
    sources: Vec<EmbeddingSourceInput>,
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

pub struct HttpEmbeddingGateway {
    api_client: ApiClient,
    authenticator: WorkerAuthenticator,
    lease_seconds: u32,
    session_ttl_seconds: u32,
    task_type: String,
    status: WorkerStatusHandle,
    worker_version: String,
    hmac_secret: String,
    session: Arc<Mutex<Option<V2SessionState>>>,
    lease_metadata: Arc<Mutex<HashMap<(u64, u64), V2LeaseMetadata>>>,
}

impl HttpEmbeddingGateway {
    pub fn new(config: &EmbeddingWorkerConfig, status: WorkerStatusHandle) -> Result<Self> {
        Ok(Self {
            api_client: ApiClient::new(config.api_url.clone())?
                .with_traffic_observer(std::sync::Arc::new(status.clone())),
            authenticator: WorkerAuthenticator::new(config.auth.clone())?,
            lease_seconds: config.lease_seconds,
            session_ttl_seconds: config.session_ttl_seconds,
            task_type: "embed.source".to_string(),
            status,
            worker_version: config.worker_version.clone(),
            hmac_secret: derive_hmac_secret(config.auth.api_key.as_str()),
            session: Arc::new(Mutex::new(None)),
            lease_metadata: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn parse_v2_claim(lease: WorkerLeaseRead) -> Result<(ClaimedEmbeddingTask, V2LeaseMetadata)> {
        let payload = serde_json::from_value::<EmbeddingTaskPayload>(lease.payload.clone())
            .map_err(|error| EmbeddingWorkerError::InvalidPayload(error.to_string()))?;
        Ok((
            ClaimedEmbeddingTask {
                task_id: lease.task_id,
                execution_id: lease.execution_id,
                job_id: payload.job_id,
                worker_version: payload.worker_version,
                sources: payload.sources,
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

    fn bearer_token(&self) -> &str {
        self.authenticator.bearer_token()
    }

    fn mark_server_connected(&self) {
        let _ = self.status.mark_server_connected();
    }

    async fn claim_once(&mut self) -> Result<Option<ClaimedEmbeddingTask>> {
        let session = self.ensure_v2_session().await?;
        let leases = self
            .api_client
            .post_json::<_, Vec<WorkerLeaseRead>>(
                "/workers/api/tasks/claim",
                &WorkerTaskClaimRequest {
                    session_id: session.session_id.clone(),
                    task_type: self.task_type.clone(),
                    worker_version: Some(self.worker_version.clone()),
                    count: 1,
                    lease_seconds: self.lease_seconds,
                },
                Some(self.bearer_token()),
            )
            .await?;
        self.mark_server_connected();
        let Some(lease) = leases.into_iter().next() else {
            return Ok(None);
        };
        let (task, mut metadata) = Self::parse_v2_claim(lease)?;
        metadata.session_id = session.session_id;
        self.lock_lease_metadata()?
            .insert((task.task_id, task.execution_id), metadata);
        Ok(Some(task))
    }
}

#[async_trait]
impl EmbeddingGateway for HttpEmbeddingGateway {
    async fn claim(&mut self) -> Result<Option<ClaimedEmbeddingTask>> {
        loop {
            match self.claim_once().await {
                Ok(task) => return Ok(task),
                Err(error) if error.is_network_error() => {
                    let _ = self
                        .status
                        .mark_server_disconnected(error.user_facing_message());
                    warn!(
                        retry_delay_seconds = NETWORK_RETRY_DELAY_SECONDS,
                        "network error while claiming embedding task, retrying: {error}"
                    );
                    sleep(Duration::from_secs(NETWORK_RETRY_DELAY_SECONDS)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn complete(
        &mut self,
        task_id: u64,
        execution_id: u64,
        worker_version: String,
        sources: Vec<EmbeddingResultSource>,
    ) -> Result<()> {
        let metadata = self
            .lock_lease_metadata()?
            .get(&(task_id, execution_id))
            .cloned()
            .ok_or_else(|| {
                EmbeddingWorkerError::Runtime(format!(
                    "missing lease metadata for task {task_id}:{execution_id}"
                ))
            })?;
        let result_payload = build_embedding_task_result_payload(sources);
        let signed_at = utc_timestamp_now();
        let nonce = new_nonce();
        let request_worker_version = Some(worker_version.clone());
        let signature_payload = json!({
            "lease_id": metadata.lease_id,
            "nonce": nonce,
            "result_payload": result_payload,
            "session_id": metadata.session_id,
            "signed_at": signed_at,
            "task_type": metadata.task_type,
            "trace_id": metadata.trace_id,
            "worker_version": request_worker_version.or(metadata.worker_version.clone()),
        });
        let signature = sign_payload(
            &self.hmac_secret,
            &signature_payload,
            CanonicalJsonMode::NormalizeExponentSign,
        )?;
        let request_body = canonical_json(
            &json!({
                "session_id": metadata.session_id.clone(),
                "lease_id": metadata.lease_id.clone(),
                "trace_id": metadata.trace_id.clone(),
                "task_type": metadata.task_type.clone(),
                "worker_version": Some(worker_version),
                "signed_at": signed_at,
                "nonce": nonce,
                "signature": signature,
                "result_payload": signature_payload["result_payload"].clone(),
            }),
            CanonicalJsonMode::NormalizeExponentSign,
        )?
        .into_bytes();

        loop {
            match self
                .api_client
                .post_json_bytes::<serde_json::Value>(
                    "/workers/api/tasks/complete",
                    request_body.clone(),
                    Some(self.bearer_token()),
                )
                .await
                .map_err(EmbeddingWorkerError::from)
            {
                Ok(_) => {
                    self.lock_lease_metadata()?.remove(&(task_id, execution_id));
                    return Ok(());
                }
                Err(error) if error.is_network_error() => {
                    let _ = self
                        .status
                        .mark_server_disconnected(error.user_facing_message());
                    warn!(
                        task_id,
                        execution_id,
                        retry_delay_seconds = NETWORK_RETRY_DELAY_SECONDS,
                        "network error while completing embedding task, retrying: {error}"
                    );
                    sleep(Duration::from_secs(NETWORK_RETRY_DELAY_SECONDS)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn fail(
        &mut self,
        task_id: u64,
        execution_id: u64,
        worker_version: String,
        error_message: String,
    ) -> Result<()> {
        let metadata = self
            .lock_lease_metadata()?
            .get(&(task_id, execution_id))
            .cloned()
            .ok_or_else(|| {
                EmbeddingWorkerError::Runtime(format!(
                    "missing lease metadata for failed task {task_id}:{execution_id}"
                ))
            })?;
        let signed_at = utc_timestamp_now();
        let nonce = new_nonce();
        let request_worker_version = Some(worker_version.clone());
        let signature_payload = json!({
            "error_message": error_message,
            "lease_id": metadata.lease_id,
            "nonce": nonce,
            "session_id": metadata.session_id,
            "signed_at": signed_at,
            "task_type": metadata.task_type,
            "trace_id": metadata.trace_id,
            "worker_version": request_worker_version.or(metadata.worker_version.clone()),
        });
        let signature = sign_payload(
            &self.hmac_secret,
            &signature_payload,
            CanonicalJsonMode::NormalizeExponentSign,
        )?;
        let request_body = canonical_json(
            &json!({
                "session_id": metadata.session_id.clone(),
                "lease_id": metadata.lease_id.clone(),
                "trace_id": metadata.trace_id.clone(),
                "task_type": metadata.task_type.clone(),
                "worker_version": Some(worker_version),
                "signed_at": signed_at,
                "nonce": nonce,
                "signature": signature,
                "error_message": signature_payload["error_message"].clone(),
            }),
            CanonicalJsonMode::NormalizeExponentSign,
        )?
        .into_bytes();

        loop {
            match self
                .api_client
                .post_json_bytes::<serde_json::Value>(
                    "/workers/api/tasks/fail",
                    request_body.clone(),
                    Some(self.bearer_token()),
                )
                .await
                .map_err(EmbeddingWorkerError::from)
            {
                Ok(_) => {
                    self.lock_lease_metadata()?.remove(&(task_id, execution_id));
                    return Ok(());
                }
                Err(error) if error.is_network_error() => {
                    let _ = self
                        .status
                        .mark_server_disconnected(error.user_facing_message());
                    warn!(
                        task_id,
                        execution_id,
                        retry_delay_seconds = NETWORK_RETRY_DELAY_SECONDS,
                        "network error while failing embedding task, retrying: {error}"
                    );
                    sleep(Duration::from_secs(NETWORK_RETRY_DELAY_SECONDS)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }
}

impl HttpEmbeddingGateway {
    async fn ensure_v2_session(&self) -> Result<V2SessionState> {
        if let Some(session) = self.current_session()? {
            if session.expires_at > Utc::now() + ChronoDuration::seconds(60) {
                return Ok(session);
            }
        }
        let session = self
            .api_client
            .post_json::<_, WorkerSessionOpenRead>(
                "/workers/api/sessions/open",
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
        self.mark_server_connected();
        Ok(next_session)
    }

    fn current_session(&self) -> Result<Option<V2SessionState>> {
        self.session
            .lock()
            .map(|session| session.clone())
            .map_err(|_| EmbeddingWorkerError::Runtime("session mutex poisoned".to_string()))
    }

    fn store_session(&self, session: V2SessionState) -> Result<()> {
        self.session
            .lock()
            .map_err(|_| EmbeddingWorkerError::Runtime("session mutex poisoned".to_string()))?
            .replace(session);
        Ok(())
    }

    fn lock_lease_metadata(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<(u64, u64), V2LeaseMetadata>>> {
        self.lease_metadata
            .lock()
            .map_err(|_| EmbeddingWorkerError::Runtime("lease metadata mutex poisoned".to_string()))
    }
}
