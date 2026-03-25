use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, WorkerError};
use crate::paths::app_paths;
use crate::types::WorkerType;

pub const WORKERS_CONFIG_SCHEMA_VERSION: u32 = 1;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RssWorkerSettings {
    pub enabled: bool,
    pub api_key: String,
    pub service_mode: ServiceMode,
    pub binary_path: Option<PathBuf>,
    pub max_in_flight_requests: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingWorkerSettings {
    pub enabled: bool,
    pub api_key: String,
    pub service_mode: ServiceMode,
    pub binary_path: Option<PathBuf>,
    pub worker_version: String,
    pub inference_batch_size: usize,
    pub acceleration_mode: AccelerationMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkersConfig {
    pub schema_version: u32,
    pub rss: RssWorkerSettings,
    pub embedding: EmbeddingWorkerSettings,
}

impl Default for RssWorkerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
            service_mode: ServiceMode::Manual,
            binary_path: None,
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
            binary_path: None,
            worker_version: DEFAULT_EMBEDDING_WORKER_VERSION.to_string(),
            inference_batch_size: DEFAULT_EMBEDDING_INFERENCE_BATCH_SIZE,
            acceleration_mode: AccelerationMode::Auto,
        }
    }
}

impl Default for WorkersConfig {
    fn default() -> Self {
        Self {
            schema_version: WORKERS_CONFIG_SCHEMA_VERSION,
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
    let config = serde_json::from_slice::<WorkersConfig>(&payload)?;
    Ok((path, config))
}

pub fn save_workers_config(path: &Path, config: &WorkersConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(config)?;
    write_secure_file(path, &payload)?;
    Ok(())
}

impl WorkersConfig {
    pub fn install_worker(
        &mut self,
        worker_type: WorkerType,
        api_key: String,
        binary_path: Option<PathBuf>,
    ) {
        match worker_type {
            WorkerType::RssScrapper => {
                self.rss.enabled = true;
                self.rss.api_key = api_key;
                self.rss.binary_path = binary_path;
            }
            WorkerType::SourceEmbedding => {
                self.embedding.enabled = true;
                self.embedding.api_key = api_key;
                self.embedding.binary_path = binary_path;
            }
        }
    }

    pub fn worker_api_url(&self, worker_type: WorkerType) -> &str {
        let _ = worker_type;
        DEFAULT_API_URL
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
