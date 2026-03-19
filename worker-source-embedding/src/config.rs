use std::env;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

use manifeed_worker_common::{WorkerAuthConfig, WorkerError, WorkerType};

use crate::error::Result;
use crate::runtime::ExecutionBackend;

const DEFAULT_API_URL: &str = "http://127.0.0.1:8000";
const DEFAULT_POLL_SECONDS: u64 = 30;
const DEFAULT_LEASE_SECONDS: u32 = 300;
const DEFAULT_INFERENCE_BATCH_SIZE: usize = 1;
const DEFAULT_HUGGINGFACE_BASE_URL: &str = "https://huggingface.co";
const DEFAULT_HUGGINGFACE_REVISION: &str = "main";
const DEFAULT_IDENTITY_DIR_SUFFIX: &str = ".config/manifeed/worker-source-embedding";
const DEFAULT_MODEL_CACHE_DIR_SUFFIX: &str = ".cache/manifeed/worker-source-embedding/models";
const DEFAULT_STATUS_FILE_SUFFIX: &str = ".local/state/manifeed/worker-source-embedding/status.json";
const DEFAULT_EXECUTION_BACKEND: ExecutionBackend = ExecutionBackend::Auto;

#[derive(Clone, Debug)]
pub struct EmbeddingWorkerConfig {
    pub api_url: String,
    pub poll_seconds: u64,
    pub lease_seconds: u32,
    pub inference_batch_size: usize,
    pub execution_backend: ExecutionBackend,
    pub ort_dylib_path: Option<PathBuf>,
    pub status_file_path: PathBuf,
    pub model_cache_dir: PathBuf,
    pub huggingface_base_url: String,
    pub huggingface_default_revision: String,
    pub huggingface_token: Option<String>,
    pub auth: WorkerAuthConfig,
}

impl EmbeddingWorkerConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            api_url: optional_env_string("MANIFEED_API_URL")
                .unwrap_or_else(|| DEFAULT_API_URL.to_string()),
            poll_seconds: env_or_default(
                "MANIFEED_EMBEDDING_POLL_SECONDS",
                DEFAULT_POLL_SECONDS,
            )?,
            lease_seconds: env_or_default(
                "MANIFEED_EMBEDDING_LEASE_SECONDS",
                DEFAULT_LEASE_SECONDS,
            )?,
            inference_batch_size: env_or_default(
                "MANIFEED_EMBEDDING_INFERENCE_BATCH_SIZE",
                DEFAULT_INFERENCE_BATCH_SIZE,
            )?,
            execution_backend: optional_env_string("MANIFEED_EMBEDDING_EXECUTION_BACKEND")
                .map(|value| value.parse::<ExecutionBackend>())
                .transpose()?
                .unwrap_or(DEFAULT_EXECUTION_BACKEND),
            ort_dylib_path: optional_env_path("MANIFEED_EMBEDDING_ORT_DYLIB_PATH"),
            status_file_path: optional_env_path("MANIFEED_EMBEDDING_STATUS_FILE")
                .unwrap_or_else(|| default_home_path(DEFAULT_STATUS_FILE_SUFFIX)),
            model_cache_dir: optional_env_path("MANIFEED_EMBEDDING_CACHE_DIR")
                .unwrap_or_else(|| default_home_path(DEFAULT_MODEL_CACHE_DIR_SUFFIX)),
            huggingface_base_url: optional_env_string("MANIFEED_EMBEDDING_HF_BASE_URL")
                .unwrap_or_else(|| DEFAULT_HUGGINGFACE_BASE_URL.to_string()),
            huggingface_default_revision: optional_env_string(
                "MANIFEED_EMBEDDING_HF_DEFAULT_REVISION",
            )
            .unwrap_or_else(|| DEFAULT_HUGGINGFACE_REVISION.to_string()),
            huggingface_token: optional_env_string("MANIFEED_EMBEDDING_HF_TOKEN")
                .or_else(|| optional_env_string("HF_TOKEN")),
            auth: WorkerAuthConfig {
                worker_type: WorkerType::SourceEmbedding,
                identity_dir: Some(
                    optional_env_path("MANIFEED_EMBEDDING_IDENTITY_DIR")
                        .unwrap_or_else(|| default_home_path(DEFAULT_IDENTITY_DIR_SUFFIX)),
                ),
                enrollment_token: std::env::var("MANIFEED_EMBEDDING_ENROLLMENT_TOKEN")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
                worker_version: env!("CARGO_PKG_VERSION").to_string(),
            },
        })
    }
}

fn optional_env_string(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn optional_env_path(key: &str) -> Option<PathBuf> {
    optional_env_string(key).map(PathBuf::from)
}

fn default_home_path(suffix: &str) -> PathBuf {
    optional_env_string("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(suffix)
}

fn env_or_default<T>(key: &str, default: T) -> Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    match optional_env_string(key) {
        Some(value) => value.parse::<T>().map_err(|error| {
            WorkerError::Config(format!("invalid value for {key}: {value} ({error})")).into()
        }),
        None => Ok(default),
    }
}
