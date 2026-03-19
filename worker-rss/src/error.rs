use manifeed_worker_common::WorkerError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, RssWorkerError>;

#[derive(Debug, Error)]
pub enum RssWorkerError {
    #[error(transparent)]
    Common(#[from] WorkerError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("invalid rss task payload: {0}")]
    InvalidPayload(String),
    #[error("invalid request header: {0}")]
    InvalidHeader(String),
    #[error("rss runtime failure: {0}")]
    Runtime(String),
}

impl RssWorkerError {
    pub fn is_network_error(&self) -> bool {
        match self {
            Self::Common(WorkerError::Http(error)) | Self::Http(error) => {
                error.is_timeout() || error.is_connect() || error.is_body() || error.is_request()
            }
            _ => false,
        }
    }
}
