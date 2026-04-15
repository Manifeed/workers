use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::time::sleep;

use crate::api::ApiClient;
use crate::auth::WorkerAuthenticator;
use crate::error::{Result, WorkerError};
use crate::protocol::{
    canonical_json, derive_hmac_secret, new_nonce, sign_payload, utc_timestamp_now,
    CanonicalJsonMode, WorkerLeaseRead, WorkerSessionOpenRead, WorkerSessionOpenRequest,
    WorkerTaskClaimRequest,
};
use crate::status::WorkerStatusHandle;

const NETWORK_RETRY_DELAY_SECONDS: u64 = 5;
const MAX_NETWORK_RETRY_ATTEMPTS: usize = 5;

#[derive(Clone)]
struct GatewayLeaseMetadata {
    pub session_id: String,
    pub lease_id: String,
    pub trace_id: String,
    pub task_type: String,
    pub worker_version: Option<String>,
}

#[derive(Clone)]
struct GatewaySessionState {
    session_id: String,
    expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct WorkerGatewayClient {
    api_client: ApiClient,
    authenticator: WorkerAuthenticator,
    lease_seconds: u32,
    session_ttl_seconds: u32,
    task_type: String,
    worker_version: String,
    hmac_secret: String,
    signature_mode: CanonicalJsonMode,
    session: Arc<Mutex<Option<GatewaySessionState>>>,
    lease_metadata: Arc<Mutex<HashMap<(u64, u64), GatewayLeaseMetadata>>>,
    status: WorkerStatusHandle,
}

impl WorkerGatewayClient {
    pub fn new(
        api_client: ApiClient,
        authenticator: WorkerAuthenticator,
        lease_seconds: u32,
        session_ttl_seconds: u32,
        task_type: impl Into<String>,
        worker_version: impl Into<String>,
        api_key: &str,
        signature_mode: CanonicalJsonMode,
        status: WorkerStatusHandle,
    ) -> Self {
        Self {
            api_client,
            authenticator,
            lease_seconds,
            session_ttl_seconds,
            task_type: task_type.into(),
            worker_version: worker_version.into(),
            hmac_secret: derive_hmac_secret(api_key),
            signature_mode,
            session: Arc::new(Mutex::new(None)),
            lease_metadata: Arc::new(Mutex::new(HashMap::new())),
            status,
        }
    }

    pub async fn claim_tasks<T, F>(&self, count: usize, mut parse_lease: F) -> Result<Vec<T>>
    where
        F: FnMut(WorkerLeaseRead) -> Result<(u64, u64, T)>,
    {
        for attempt in 0..MAX_NETWORK_RETRY_ATTEMPTS {
            match self.claim_tasks_once(count, &mut parse_lease).await {
                Ok(tasks) => return Ok(tasks),
                Err(error)
                    if is_retryable_network_error(&error)
                        && attempt + 1 < MAX_NETWORK_RETRY_ATTEMPTS =>
                {
                    let _ = self
                        .status
                        .mark_server_disconnected(crate::error::user_facing_error_message(&error));
                    sleep(Duration::from_secs(NETWORK_RETRY_DELAY_SECONDS)).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(WorkerError::Process(
            "gateway retries exhausted".to_string(),
        ))
    }

    pub async fn complete_task<T>(
        &self,
        task_id: u64,
        execution_id: u64,
        result_payload: &T,
    ) -> Result<()>
    where
        T: Serialize,
    {
        let metadata = self.require_lease_metadata(task_id, execution_id)?;
        let result_payload = serde_json::to_value(result_payload)?;
        let request_body = self.build_complete_request_body(&metadata, &result_payload)?;
        self.send_result_request(
            task_id,
            execution_id,
            "/workers/api/tasks/complete",
            request_body,
        )
        .await
    }

    pub async fn fail_task(
        &self,
        task_id: u64,
        execution_id: u64,
        error_message: &str,
    ) -> Result<()> {
        let metadata = self.require_lease_metadata(task_id, execution_id)?;
        let request_body = self.build_fail_request_body(&metadata, error_message)?;
        self.send_result_request(
            task_id,
            execution_id,
            "/workers/api/tasks/fail",
            request_body,
        )
        .await
    }

    fn bearer_token(&self) -> &str {
        self.authenticator.bearer_token()
    }

    async fn claim_tasks_once<T, F>(&self, count: usize, parse_lease: &mut F) -> Result<Vec<T>>
    where
        F: FnMut(WorkerLeaseRead) -> Result<(u64, u64, T)>,
    {
        let session = self.ensure_session().await?;
        let leases = self
            .api_client
            .post_json::<_, Vec<WorkerLeaseRead>>(
                "/workers/api/tasks/claim",
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

        let mut tasks = Vec::with_capacity(leases.len());
        let mut lease_metadata = self.lock_lease_metadata()?;
        for lease in leases {
            let metadata = GatewayLeaseMetadata {
                session_id: session.session_id.clone(),
                lease_id: lease.lease_id.clone(),
                trace_id: lease.trace_id.clone(),
                task_type: lease.task_type.clone(),
                worker_version: lease.worker_version.clone(),
            };
            let (task_id, execution_id, task) = parse_lease(lease)?;
            lease_metadata.insert((task_id, execution_id), metadata);
            tasks.push(task);
        }
        let _ = self.status.mark_server_connected();
        Ok(tasks)
    }

    async fn send_result_request(
        &self,
        task_id: u64,
        execution_id: u64,
        path: &str,
        request_body: Vec<u8>,
    ) -> Result<()> {
        for attempt in 0..MAX_NETWORK_RETRY_ATTEMPTS {
            match self
                .api_client
                .post_json_bytes::<serde_json::Value>(
                    path,
                    request_body.clone(),
                    Some(self.bearer_token()),
                )
                .await
            {
                Ok(_) => {
                    self.remove_lease_metadata(task_id, execution_id)?;
                    let _ = self.status.mark_server_connected();
                    return Ok(());
                }
                Err(error)
                    if is_retryable_network_error(&error)
                        && attempt + 1 < MAX_NETWORK_RETRY_ATTEMPTS =>
                {
                    let _ = self
                        .status
                        .mark_server_disconnected(crate::error::user_facing_error_message(&error));
                    sleep(Duration::from_secs(NETWORK_RETRY_DELAY_SECONDS)).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(WorkerError::Process(
            "gateway retries exhausted".to_string(),
        ))
    }

    async fn ensure_session(&self) -> Result<GatewaySessionState> {
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
        let next_session = GatewaySessionState {
            session_id: session.session_id,
            expires_at: session.expires_at,
        };
        self.store_session(next_session.clone())?;
        let _ = self.status.mark_server_connected();
        Ok(next_session)
    }

    fn build_complete_request_body(
        &self,
        metadata: &GatewayLeaseMetadata,
        result_payload: &Value,
    ) -> Result<Vec<u8>> {
        let signed_at = utc_timestamp_now();
        let nonce = new_nonce();
        let signature_payload = json!({
            "lease_id": metadata.lease_id,
            "nonce": nonce,
            "result_payload": result_payload,
            "session_id": metadata.session_id,
            "signed_at": signed_at,
            "task_type": metadata.task_type,
            "trace_id": metadata.trace_id,
            "worker_version": metadata.worker_version,
        });
        let signature = sign_payload(&self.hmac_secret, &signature_payload, self.signature_mode)?;
        Ok(canonical_json(
            &json!({
                "session_id": metadata.session_id,
                "lease_id": metadata.lease_id,
                "trace_id": metadata.trace_id,
                "task_type": metadata.task_type,
                "worker_version": metadata.worker_version,
                "signed_at": signed_at,
                "nonce": nonce,
                "signature": signature,
                "result_payload": result_payload,
            }),
            self.signature_mode,
        )?
        .into_bytes())
    }

    fn build_fail_request_body(
        &self,
        metadata: &GatewayLeaseMetadata,
        error_message: &str,
    ) -> Result<Vec<u8>> {
        let signed_at = utc_timestamp_now();
        let nonce = new_nonce();
        let signature_payload = json!({
            "error_message": error_message,
            "lease_id": metadata.lease_id,
            "nonce": nonce,
            "session_id": metadata.session_id,
            "signed_at": signed_at,
            "task_type": metadata.task_type,
            "trace_id": metadata.trace_id,
            "worker_version": metadata.worker_version,
        });
        let signature = sign_payload(&self.hmac_secret, &signature_payload, self.signature_mode)?;
        Ok(canonical_json(
            &json!({
                "session_id": metadata.session_id,
                "lease_id": metadata.lease_id,
                "trace_id": metadata.trace_id,
                "task_type": metadata.task_type,
                "worker_version": metadata.worker_version,
                "signed_at": signed_at,
                "nonce": nonce,
                "signature": signature,
                "error_message": error_message,
            }),
            self.signature_mode,
        )?
        .into_bytes())
    }

    fn require_lease_metadata(
        &self,
        task_id: u64,
        execution_id: u64,
    ) -> Result<GatewayLeaseMetadata> {
        self.lock_lease_metadata()?
            .get(&(task_id, execution_id))
            .cloned()
            .ok_or_else(|| {
                WorkerError::Process(format!(
                    "missing lease metadata for task {task_id}:{execution_id}"
                ))
            })
    }

    fn remove_lease_metadata(&self, task_id: u64, execution_id: u64) -> Result<()> {
        self.lock_lease_metadata()?.remove(&(task_id, execution_id));
        Ok(())
    }

    fn current_session(&self) -> Result<Option<GatewaySessionState>> {
        self.session
            .lock()
            .map(|session| session.clone())
            .map_err(|_| WorkerError::Process("session mutex poisoned".to_string()))
    }

    fn store_session(&self, session: GatewaySessionState) -> Result<()> {
        self.session
            .lock()
            .map_err(|_| WorkerError::Process("session mutex poisoned".to_string()))?
            .replace(session);
        Ok(())
    }

    fn lock_lease_metadata(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<(u64, u64), GatewayLeaseMetadata>>> {
        self.lease_metadata
            .lock()
            .map_err(|_| WorkerError::Process("lease metadata mutex poisoned".to_string()))
    }
}

fn is_retryable_network_error(error: &WorkerError) -> bool {
    matches!(error, WorkerError::Http(_))
}
