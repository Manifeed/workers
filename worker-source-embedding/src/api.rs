use async_trait::async_trait;
use manifeed_worker_common::{
    ApiClient, CanonicalJsonMode, WorkerAuthenticator, WorkerError, WorkerGatewayClient,
    WorkerLeaseRead, WorkerStatusHandle,
};
use serde::Deserialize;

use crate::config::EmbeddingWorkerConfig;
use crate::error::{EmbeddingWorkerError, Result};
use crate::gateway::build_embedding_task_result_payload;
use crate::worker::{
    ClaimedEmbeddingTask, EmbeddingGateway, EmbeddingResultSource, EmbeddingSourceInput,
};

#[derive(Deserialize)]
struct EmbeddingTaskPayload {
    job_id: String,
    worker_version: String,
    sources: Vec<EmbeddingSourceInput>,
}

pub struct HttpEmbeddingGateway {
    client: WorkerGatewayClient,
}

impl HttpEmbeddingGateway {
    pub fn new(config: &EmbeddingWorkerConfig, status: WorkerStatusHandle) -> Result<Self> {
        let api_client = ApiClient::new(config.api_url.clone())?
            .with_traffic_observer(std::sync::Arc::new(status.clone()));
        let authenticator = WorkerAuthenticator::new(config.auth.clone())?;
        Ok(Self {
            client: WorkerGatewayClient::new(
                api_client,
                authenticator,
                config.lease_seconds,
                config.session_ttl_seconds,
                "embed.source",
                config.worker_version.clone(),
                config.auth.api_key.as_str(),
                CanonicalJsonMode::NormalizeExponentSign,
                status,
            ),
        })
    }

    fn parse_claim(
        lease: WorkerLeaseRead,
    ) -> std::result::Result<(u64, u64, ClaimedEmbeddingTask), WorkerError> {
        let payload = serde_json::from_value::<EmbeddingTaskPayload>(lease.payload)
            .map_err(|error| WorkerError::ResponseDecode(error.to_string()))?;
        Ok((
            lease.task_id,
            lease.execution_id,
            ClaimedEmbeddingTask {
                task_id: lease.task_id,
                execution_id: lease.execution_id,
                job_id: payload.job_id,
                worker_version: payload.worker_version,
                sources: payload.sources,
            },
        ))
    }
}

#[async_trait]
impl EmbeddingGateway for HttpEmbeddingGateway {
    async fn claim(&mut self) -> Result<Option<ClaimedEmbeddingTask>> {
        let mut tasks = self
            .client
            .claim_tasks(1, Self::parse_claim)
            .await
            .map_err(EmbeddingWorkerError::from)?;
        Ok(tasks.pop())
    }

    async fn complete(
        &mut self,
        task_id: u64,
        execution_id: u64,
        _worker_version: String,
        sources: Vec<EmbeddingResultSource>,
    ) -> Result<()> {
        let payload = build_embedding_task_result_payload(sources);
        self.client
            .complete_task(task_id, execution_id, &payload)
            .await
            .map_err(EmbeddingWorkerError::from)
    }

    async fn fail(
        &mut self,
        task_id: u64,
        execution_id: u64,
        _worker_version: String,
        error_message: String,
    ) -> Result<()> {
        self.client
            .fail_task(task_id, execution_id, &error_message)
            .await
            .map_err(EmbeddingWorkerError::from)
    }
}
