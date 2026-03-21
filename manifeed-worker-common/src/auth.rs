use hostname::get;

use crate::error::{Result, WorkerError};
use crate::types::WorkerType;

#[derive(Clone, Debug)]
pub struct WorkerAuthConfig {
    pub worker_type: WorkerType,
    pub api_key: String,
    pub worker_name: String,
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

        let worker_name = config.worker_name.trim().to_string();
        if worker_name.is_empty() {
            return Err(WorkerError::Config(
                "worker name resolved to an empty value".to_string(),
            ));
        }

        Ok(Self {
            config: WorkerAuthConfig {
                worker_type: config.worker_type,
                api_key,
                worker_name,
            },
        })
    }

    pub fn bearer_token(&self) -> &str {
        &self.config.api_key
    }

    pub fn worker_name(&self) -> &str {
        &self.config.worker_name
    }
}

pub fn resolve_worker_name(configured_name: Option<String>, worker_type: WorkerType) -> String {
    configured_name
        .and_then(normalize_worker_name)
        .or_else(|| get().ok().and_then(|hostname| hostname.into_string().ok()))
        .and_then(normalize_worker_name)
        .unwrap_or_else(|| format!("{}-worker", worker_type.as_str()))
}

fn normalize_worker_name(value: String) -> Option<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}
