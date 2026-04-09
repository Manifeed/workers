use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, WorkerError};
use crate::paths::app_paths;
use crate::types::WorkerType;

pub const WORKERS_CONFIG_SCHEMA_VERSION: u32 = 4;
pub const DEFAULT_API_URL: &str = "http://127.0.0.1:8000";

pub const DEFAULT_RSS_POLL_SECONDS: u64 = 60;
pub const DEFAULT_RSS_LEASE_SECONDS: u32 = 300;
pub const DEFAULT_RSS_HOST_MAX_REQUESTS_PER_SECOND: u32 = 20;
pub const DEFAULT_RSS_MAX_IN_FLIGHT_REQUESTS: usize = 5;
pub const DEFAULT_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST: usize = 5;
pub const DEFAULT_RSS_MAX_CLAIMED_TASKS: usize = 5;
pub const DEFAULT_RSS_REQUEST_TIMEOUT_SECONDS: u64 = 10;
pub const DEFAULT_RSS_FETCH_RETRY_COUNT: u32 = 1;

pub const DEFAULT_EMBEDDING_POLL_SECONDS: u64 = 30;
pub const DEFAULT_EMBEDDING_LEASE_SECONDS: u32 = 300;
pub const DEFAULT_EMBEDDING_INFERENCE_BATCH_SIZE: usize = 1;
pub const DEFAULT_EMBEDDING_WORKER_VERSION: &str = "e5-large-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceMode {
    Manual,
    Background,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccelerationMode {
    Auto,
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingRuntimeBundle {
    None,
    Cuda12,
    WebGpu,
    CoreMl,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RssWorkerSettings {
    pub enabled: bool,
    pub api_key: String,
    pub service_mode: ServiceMode,
    pub installed_version: String,
    pub max_in_flight_requests: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingWorkerSettings {
    pub enabled: bool,
    pub api_key: String,
    pub service_mode: ServiceMode,
    pub installed_version: String,
    pub worker_version: String,
    pub inference_batch_size: usize,
    pub acceleration_mode: AccelerationMode,
    pub runtime_bundle: EmbeddingRuntimeBundle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkersConfig {
    pub schema_version: u32,
    pub api_url: String,
    pub desktop_installed_version: String,
    pub rss: RssWorkerSettings,
    pub embedding: EmbeddingWorkerSettings,
}

impl Default for RssWorkerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
            service_mode: ServiceMode::Manual,
            installed_version: String::new(),
            max_in_flight_requests: DEFAULT_RSS_MAX_IN_FLIGHT_REQUESTS,
        }
    }
}

impl Default for EmbeddingWorkerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
            service_mode: ServiceMode::Manual,
            installed_version: String::new(),
            worker_version: DEFAULT_EMBEDDING_WORKER_VERSION.to_string(),
            inference_batch_size: DEFAULT_EMBEDDING_INFERENCE_BATCH_SIZE,
            acceleration_mode: AccelerationMode::Auto,
            runtime_bundle: EmbeddingRuntimeBundle::None,
        }
    }
}

impl Default for WorkersConfig {
    fn default() -> Self {
        Self {
            schema_version: WORKERS_CONFIG_SCHEMA_VERSION,
            api_url: DEFAULT_API_URL.to_string(),
            desktop_installed_version: String::new(),
            rss: RssWorkerSettings::default(),
            embedding: EmbeddingWorkerSettings::default(),
        }
    }
}

pub fn resolve_workers_config_path(explicit_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit_path {
        return Ok(path.to_path_buf());
    }
    Ok(app_paths()?.workers_config_file())
}

pub fn load_workers_config(explicit_path: Option<&Path>) -> Result<(PathBuf, WorkersConfig)> {
    let path = resolve_workers_config_path(explicit_path)?;
    if !path.exists() {
        return Ok((path, WorkersConfig::default()));
    }

    let payload = fs::read(&path)?;
    let mut config = serde_json::from_slice::<WorkersConfig>(&payload)?;
    config.schema_version = WORKERS_CONFIG_SCHEMA_VERSION;
    if config.api_url.trim().is_empty() {
        config.api_url = DEFAULT_API_URL.to_string();
    }
    Ok((path, config))
}

pub fn save_workers_config(path: &Path, config: &WorkersConfig) -> Result<()> {
    let mut normalized = config.clone();
    normalized.schema_version = WORKERS_CONFIG_SCHEMA_VERSION;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(&normalized)?;
    write_secure_file(path, &payload)?;
    Ok(())
}

impl WorkersConfig {
    pub fn worker_api_url(&self, worker_type: WorkerType) -> &str {
        let _ = worker_type;
        if self.api_url.trim().is_empty() {
            DEFAULT_API_URL
        } else {
            self.api_url.as_str()
        }
    }

    pub fn worker_api_key(&self, worker_type: WorkerType) -> &str {
        match worker_type {
            WorkerType::RssScrapper => &self.rss.api_key,
            WorkerType::SourceEmbedding => &self.embedding.api_key,
        }
    }
}

fn write_secure_file(path: &Path, payload: &[u8]) -> Result<()> {
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, payload)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temp_path, path).map_err(|error| {
        WorkerError::Io(std::io::Error::new(
            error.kind(),
            format!(
                "unable to move config file into place {}: {error}",
                path.display()
            ),
        ))
    })?;
    Ok(())
}
