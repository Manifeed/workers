use manifeed_worker_common::{
    is_auth_error, user_facing_error_message, user_facing_reqwest_error_message, WorkerError,
};
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

    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::Common(error) if is_auth_error(error))
    }

    pub fn user_facing_message(&self) -> String {
        match self {
            Self::Common(error) => user_facing_error_message(error),
            Self::Http(error) => user_facing_reqwest_error_message(error),
            Self::InvalidPayload(_) => "Invalid task data".to_string(),
            Self::InvalidHeader(_) => "Invalid API request".to_string(),
            Self::Runtime(_) => "Worker runtime error".to_string(),
        }
    }
}
