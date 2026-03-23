use crate::error::{Result, WorkerError};
use crate::types::WorkerType;

#[derive(Clone, Debug)]
pub struct WorkerAuthConfig {
    pub worker_type: WorkerType,
    pub api_key: String,
}

#[derive(Clone, Debug)]
pub struct WorkerAuthenticator {
    config: WorkerAuthConfig,
}

impl WorkerAuthenticator {
    pub fn new(config: WorkerAuthConfig) -> Result<Self> {
        let api_key = config.api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(WorkerError::Config(
                "MANIFEED_WORKER_API_KEY is empty".to_string(),
            ));
        }

        Ok(Self {
            config: WorkerAuthConfig {
                worker_type: config.worker_type,
                api_key,
            },
        })
    }

    pub fn bearer_token(&self) -> &str {
        &self.config.api_key
    }
}
