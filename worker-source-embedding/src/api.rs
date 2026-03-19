use std::time::Duration;

use async_trait::async_trait;
use manifeed_worker_common::{
    ApiClient, WorkerAuthenticator, WorkerTaskClaim, WorkerTaskClaimRequest,
};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::warn;

use crate::config::EmbeddingWorkerConfig;
use crate::error::{EmbeddingWorkerError, Result};
use crate::status::WorkerStatusHandle;
use crate::worker::{
    ClaimedEmbeddingTask, EmbeddingGateway, EmbeddingResultSource, EmbeddingSourceInput,
};

const NETWORK_RETRY_DELAY_SECONDS: u64 = 5;

#[derive(Deserialize)]
struct EmbeddingTaskPayload {
    job_id: String,
    embedding_model_name: String,
    sources: Vec<EmbeddingSourceInput>,
}

#[derive(Clone, Serialize)]
struct EmbeddingTaskCompleteRequest {
    task_id: u64,
    execution_id: u64,
    result_payload: EmbeddingTaskCompletePayload,
}

#[derive(Clone, Serialize)]
struct EmbeddingTaskCompletePayload {
    sources: Vec<EmbeddingResultSource>,
}

#[derive(Clone, Serialize)]
struct EmbeddingTaskFailRequest {
    task_id: u64,
    execution_id: u64,
    error_message: String,
}

pub struct HttpEmbeddingGateway {
    api_client: ApiClient,
    authenticator: WorkerAuthenticator,
    lease_seconds: u32,
    status: WorkerStatusHandle,
}

impl HttpEmbeddingGateway {
    pub fn new(config: &EmbeddingWorkerConfig, status: WorkerStatusHandle) -> Result<Self> {
        Ok(Self {
            api_client: ApiClient::new(config.api_url.clone())?
                .with_traffic_observer(std::sync::Arc::new(status.clone())),
            authenticator: WorkerAuthenticator::new(config.auth.clone())?,
            lease_seconds: config.lease_seconds,
            status,
        })
    }

    fn parse_claim(task: WorkerTaskClaim) -> Result<ClaimedEmbeddingTask> {
        let payload = serde_json::from_value::<EmbeddingTaskPayload>(task.payload)
            .map_err(|error| EmbeddingWorkerError::InvalidPayload(error.to_string()))?;
        Ok(ClaimedEmbeddingTask {
            task_id: task.task_id,
            execution_id: task.execution_id,
            job_id: payload.job_id,
            embedding_model_name: payload.embedding_model_name,
            sources: payload.sources,
        })
    }

    async fn bearer_token(&mut self) -> Result<String> {
        let token = self.authenticator.ensure_session(&self.api_client).await?;
        let _ = self.status.mark_server_connected();
        Ok(token)
    }

    async fn claim_once(&mut self) -> Result<Option<ClaimedEmbeddingTask>> {
        let token = self.bearer_token().await?;
        let tasks = self
            .api_client
            .post_json::<_, Vec<WorkerTaskClaim>>(
                "/workers/embedding/claim",
                &WorkerTaskClaimRequest {
                    count: 1,
                    lease_seconds: self.lease_seconds,
                },
                Some(&token),
            )
            .await?;
        let _ = self.status.mark_server_connected();
        tasks.into_iter().next().map(Self::parse_claim).transpose()
    }

    async fn complete_once(&mut self, request: &EmbeddingTaskCompleteRequest) -> Result<()> {
        let token = self.bearer_token().await?;
        self.api_client
            .post_json::<_, serde_json::Value>(
                "/workers/embedding/complete",
                request,
                Some(&token),
            )
            .await?;
        let _ = self.status.mark_server_connected();
        Ok(())
    }

    async fn fail_once(&mut self, request: &EmbeddingTaskFailRequest) -> Result<()> {
        let token = self.bearer_token().await?;
        self.api_client
            .post_json::<_, serde_json::Value>(
                "/workers/embedding/fail",
                request,
                Some(&token),
            )
            .await?;
        let _ = self.status.mark_server_connected();
        Ok(())
    }
}

#[async_trait]
impl EmbeddingGateway for HttpEmbeddingGateway {
    async fn claim(&mut self) -> Result<Option<ClaimedEmbeddingTask>> {
        loop {
            match self.claim_once().await {
                Ok(task) => return Ok(task),
                Err(error) if error.is_network_error() => {
                    let _ = self.status.mark_server_disconnected(error.to_string());
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
        sources: Vec<EmbeddingResultSource>,
    ) -> Result<()> {
        let request = EmbeddingTaskCompleteRequest {
            task_id,
            execution_id,
            result_payload: EmbeddingTaskCompletePayload { sources },
        };

        loop {
            match self.complete_once(&request).await {
                Ok(()) => return Ok(()),
                Err(error) if error.is_network_error() => {
                    let _ = self.status.mark_server_disconnected(error.to_string());
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

    async fn fail(&mut self, task_id: u64, execution_id: u64, error_message: String) -> Result<()> {
        let request = EmbeddingTaskFailRequest {
            task_id,
            execution_id,
            error_message,
        };

        loop {
            match self.fail_once(&request).await {
                Ok(()) => return Ok(()),
                Err(error) if error.is_network_error() => {
                    let _ = self.status.mark_server_disconnected(error.to_string());
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
