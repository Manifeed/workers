use manifeed_worker_common::{is_auth_error, user_facing_error_message, WorkerError};
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

    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::Common(error) if is_auth_error(error))
    }

    pub fn user_facing_message(&self) -> String {
        match self {
            Self::Common(error) => user_facing_error_message(error),
            Self::InvalidPayload(_) => "Invalid task data".to_string(),
            Self::InvalidModelReference(_) => "Invalid model config".to_string(),
            Self::MissingModelArtifact { .. } => "Missing model files".to_string(),
            Self::Runtime(_) => "Worker runtime error".to_string(),
        }
    }
}
