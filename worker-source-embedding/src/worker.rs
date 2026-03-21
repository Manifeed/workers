use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{EmbeddingWorkerError, Result};
use crate::status::WorkerStatusHandle;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmbeddingSourceInput {
    pub id: u64,
    pub title: String,
    pub summary: Option<String>,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct ClaimedEmbeddingTask {
    pub task_id: u64,
    pub execution_id: u64,
    pub job_id: String,
    pub embedding_model_name: String,
    pub sources: Vec<EmbeddingSourceInput>,
}

#[async_trait]
pub trait EmbeddingGateway {
    async fn claim(&mut self) -> Result<Option<ClaimedEmbeddingTask>>;
    async fn complete(
        &mut self,
        task_id: u64,
        execution_id: u64,
        sources: Vec<EmbeddingResultSource>,
    ) -> Result<()>;
    async fn fail(&mut self, task_id: u64, execution_id: u64, error_message: String) -> Result<()>;
}

#[async_trait]
pub trait ModelEmbedder {
    async fn embed(&mut self, model_name: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>>;
}

#[derive(Clone, Debug, Serialize)]
pub struct EmbeddingResultSource {
    pub id: u64,
    pub embedding: Vec<f32>,
}

pub struct EmbeddingWorker<G, E> {
    gateway: G,
    embedder: E,
    inference_batch_size: usize,
    status: WorkerStatusHandle,
}

impl<G, E> EmbeddingWorker<G, E>
where
    G: EmbeddingGateway,
    E: ModelEmbedder,
{
    pub fn new(
        gateway: G,
        embedder: E,
        inference_batch_size: usize,
        status: WorkerStatusHandle,
    ) -> Self {
        Self {
            gateway,
            embedder,
            inference_batch_size: inference_batch_size.max(1),
            status,
        }
    }

    pub async fn run_once(&mut self) -> Result<bool> {
        let Some(task) = self.gateway.claim().await? else {
            let _ = self.status.mark_idle();
            return Ok(false);
        };
        let _ = self.status.mark_processing(&task);

        let mut vectors = Vec::with_capacity(task.sources.len());
        let mut chunk_inputs = Vec::with_capacity(self.inference_batch_size);
        for source_batch in task.sources.chunks(self.inference_batch_size) {
            chunk_inputs.clear();
            chunk_inputs.extend(
                source_batch
                    .iter()
                    .map(|source| build_embedding_input(&task.embedding_model_name, source)),
            );
            let mut batch_vectors = match self
                .embedder
                .embed(&task.embedding_model_name, &chunk_inputs)
                .await
            {
                Ok(vectors) => vectors,
                Err(error) => {
                    let _ = self.status.mark_error(error.to_string());
                    self.gateway
                        .fail(task.task_id, task.execution_id, error.to_string())
                        .await?;
                    return Err(error);
                }
            };
            vectors.append(&mut batch_vectors);
        }

        if vectors.len() != task.sources.len() {
            let message = format!(
                "embedding count mismatch for task {}: expected {}, got {}",
                task.task_id,
                task.sources.len(),
                vectors.len()
            );
            let _ = self.status.mark_error(message.clone());
            self.gateway
                .fail(task.task_id, task.execution_id, message.clone())
                .await?;
            return Err(EmbeddingWorkerError::Runtime(message));
        }

        let results = task
            .sources
            .iter()
            .zip(vectors.into_iter())
            .map(|(source, embedding)| EmbeddingResultSource {
                id: source.id,
                embedding,
            })
            .collect::<Vec<_>>();
        self.gateway
            .complete(task.task_id, task.execution_id, results)
            .await?;
        let _ = self.status.mark_completed_task();
        Ok(true)
    }
}

pub fn build_embedding_input(model_name: &str, source: &EmbeddingSourceInput) -> String {
    if uses_e5_passage_prefix(model_name) {
        return build_e5_passage_input(source);
    }
    build_plain_embedding_input(source)
}

fn build_e5_passage_input(source: &EmbeddingSourceInput) -> String {
    format!("passage: {}", build_plain_embedding_input(source))
}

fn build_plain_embedding_input(source: &EmbeddingSourceInput) -> String {
    let mut parts = vec![format!("title: {}", normalize_whitespace(&source.title))];
    if let Some(summary) = source
        .summary
        .as_ref()
        .map(|value| normalize_whitespace(value))
    {
        if !summary.is_empty() {
            parts.push(format!("summary: {summary}"));
        }
    }
    parts.join(" | ")
}

fn uses_e5_passage_prefix(model_name: &str) -> bool {
    model_name.to_ascii_lowercase().contains("e5")
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
