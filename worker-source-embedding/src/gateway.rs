use serde::{Deserialize, Serialize};

use crate::worker::EmbeddingResultSource;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerEmbeddingTaskResultPayload {
    pub sources: Vec<EmbeddingResultSource>,
}

pub fn build_embedding_task_result_payload(
    sources: Vec<EmbeddingResultSource>,
) -> WorkerEmbeddingTaskResultPayload {
    WorkerEmbeddingTaskResultPayload { sources }
}
