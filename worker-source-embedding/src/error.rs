use manifeed_worker_common::WorkerError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, EmbeddingWorkerError>;

#[derive(Debug, Error)]
pub enum EmbeddingWorkerError {
    #[error(transparent)]
    Common(#[from] WorkerError),
    #[error("invalid embedding payload: {0}")]
    InvalidPayload(String),
    #[error("invalid hugging face model reference: {0}")]
    InvalidModelReference(String),
    #[error("missing required onnx artifact for {model_name}: {artifact_name}")]
    MissingModelArtifact {
        model_name: String,
        artifact_name: String,
    },
    #[error("embedding runtime failure: {0}")]
    Runtime(String),
}

impl EmbeddingWorkerError {
    pub fn is_network_error(&self) -> bool {
        matches!(self, Self::Common(WorkerError::Http(_)))
    }
}
