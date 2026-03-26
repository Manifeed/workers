use std::env;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

use manifeed_worker_common::{
    app_paths, load_workers_config, AccelerationMode, WorkerAuthConfig, WorkerError, WorkerType,
    DEFAULT_API_URL, DEFAULT_EMBEDDING_WORKER_VERSION,
};

use crate::error::Result;
use crate::runtime::{probe_system, ExecutionBackend};

const DEFAULT_HUGGINGFACE_BASE_URL: &str = "https://huggingface.co";
const DEFAULT_HUGGINGFACE_REVISION: &str = "main";
pub const FIXED_EMBEDDING_MODEL_NAME: &str = "Xenova/multilingual-e5-large";
const BUILTIN_EMBEDDING_POLL_SECONDS: u64 = 30;
const BUILTIN_EMBEDDING_LEASE_SECONDS: u32 = 300;

#[derive(Clone, Debug, Default)]
pub struct EmbeddingWorkerConfigOverrides {
    pub config_path: Option<PathBuf>,
    pub api_key: Option<String>,
    pub acceleration_mode: Option<AccelerationMode>,
    pub provider_override: Option<ExecutionBackend>,
}

#[derive(Clone, Debug)]
pub struct EmbeddingWorkerConfig {
    pub api_url: String,
    pub poll_seconds: u64,
    pub lease_seconds: u32,
    pub session_ttl_seconds: u32,
    pub inference_batch_size: usize,
    pub acceleration_mode: AccelerationMode,
    pub execution_backend: ExecutionBackend,
    pub ort_dylib_path: Option<PathBuf>,
    pub status_file_path: PathBuf,
    pub version_cache_path: PathBuf,
    pub config_path: PathBuf,
    pub model_cache_dir: PathBuf,
    pub huggingface_base_url: String,
    pub huggingface_default_revision: String,
    pub huggingface_token: Option<String>,
    pub worker_version: String,
    pub auth: WorkerAuthConfig,
}

impl EmbeddingWorkerConfig {
    pub fn load(overrides: EmbeddingWorkerConfigOverrides) -> Result<Self> {
        let app_dirs = app_paths()?;
        let worker_paths = app_dirs.worker_paths(WorkerType::SourceEmbedding);
        let (config_path, stored) = load_workers_config(overrides.config_path.as_deref())?;
        let acceleration_mode = overrides
            .acceleration_mode
            .or_else(|| {
                optional_env_string("MANIFEED_EMBEDDING_ACCELERATION_MODE")
                    .and_then(parse_acceleration_mode)
            })
            .unwrap_or(stored.embedding.acceleration_mode);
        let derived_ort_dylib_path = worker_paths
            .install_dir
            .join("runtime/lib/libonnxruntime.so");
        let ort_dylib_path = optional_env_path("MANIFEED_EMBEDDING_ORT_DYLIB_PATH").or_else(|| {
            derived_ort_dylib_path
                .exists()
                .then_some(derived_ort_dylib_path.clone())
        });
        let execution_backend = resolve_execution_backend(
            acceleration_mode,
            overrides.provider_override.or_else(|| {
                optional_env_string("MANIFEED_EMBEDDING_EXECUTION_BACKEND")
                    .map(|value| value.parse::<ExecutionBackend>())
                    .transpose()
                    .ok()
                    .flatten()
            }),
            ort_dylib_path.as_deref(),
        )?;
        let api_key = overrides
            .api_key
            .or_else(|| optional_env_string("MANIFEED_WORKER_API_KEY"))
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                (!stored.embedding.api_key.trim().is_empty()).then(|| stored.embedding.api_key.clone())
            })
            .ok_or_else(|| {
                WorkerError::Config(
                    "missing worker API key; run `worker-source-embedding install --api-key ...` or set MANIFEED_WORKER_API_KEY"
                        .to_string(),
                )
            })?;

        Ok(Self {
            api_url: DEFAULT_API_URL.to_string(),
            poll_seconds: BUILTIN_EMBEDDING_POLL_SECONDS,
            lease_seconds: BUILTIN_EMBEDDING_LEASE_SECONDS,
            session_ttl_seconds: env_or_value(
                "MANIFEED_EMBEDDING_SESSION_TTL_SECONDS",
                3600u32,
            )?,
            inference_batch_size: stored.embedding.inference_batch_size.max(1),
            acceleration_mode,
            execution_backend,
            ort_dylib_path,
            status_file_path: worker_paths.status_file,
            version_cache_path: app_dirs.version_cache_dir().join(format!(
                "{}.json",
                WorkerType::SourceEmbedding.cli_product()
            )),
            config_path,
            model_cache_dir: optional_env_path("MANIFEED_EMBEDDING_CACHE_DIR")
                .unwrap_or(worker_paths.cache_dir),
            huggingface_base_url: optional_env_string("MANIFEED_EMBEDDING_HF_BASE_URL")
                .unwrap_or_else(|| DEFAULT_HUGGINGFACE_BASE_URL.to_string()),
            huggingface_default_revision: optional_env_string(
                "MANIFEED_EMBEDDING_HF_DEFAULT_REVISION",
            )
            .unwrap_or_else(|| DEFAULT_HUGGINGFACE_REVISION.to_string()),
            huggingface_token: optional_env_string("MANIFEED_EMBEDDING_HF_TOKEN")
                .or_else(|| optional_env_string("HF_TOKEN")),
            worker_version: optional_env_string("MANIFEED_EMBEDDING_WORKER_VERSION")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    if stored.embedding.worker_version.trim().is_empty() {
                        DEFAULT_EMBEDDING_WORKER_VERSION.to_string()
                    } else {
                        stored.embedding.worker_version.clone()
                    }
                }),
            auth: WorkerAuthConfig {
                worker_type: WorkerType::SourceEmbedding,
                api_key,
            },
        })
    }
}

fn resolve_execution_backend(
    acceleration_mode: AccelerationMode,
    provider_override: Option<ExecutionBackend>,
    ort_dylib_path: Option<&std::path::Path>,
) -> Result<ExecutionBackend> {
    if let Some(provider_override) = provider_override {
        return Ok(provider_override);
    }

    match acceleration_mode {
        AccelerationMode::Auto => Ok(ExecutionBackend::Auto),
        AccelerationMode::Cpu => Ok(ExecutionBackend::Cpu),
        AccelerationMode::Gpu => {
            let probe = probe_system(ExecutionBackend::Auto, ort_dylib_path);
            match probe.recommended_backend {
                ExecutionBackend::Cuda | ExecutionBackend::WebGpu => Ok(probe.recommended_backend),
                _ => Err(WorkerError::Config(
                    "gpu acceleration requested but no supported GPU execution backend was detected"
                        .to_string(),
                )
                .into()),
            }
        }
    }
}

fn parse_acceleration_mode(value: String) -> Option<AccelerationMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(AccelerationMode::Auto),
        "cpu" => Some(AccelerationMode::Cpu),
        "gpu" => Some(AccelerationMode::Gpu),
        _ => None,
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

fn env_or_value<T>(key: &str, value: T) -> Result<T>
where
    T: Clone + FromStr,
    T::Err: Display,
{
    match optional_env_string(key) {
        Some(raw) => raw.parse::<T>().map_err(|error| {
            WorkerError::Config(format!("invalid value for {key}: {raw} ({error})")).into()
        }),
        None => Ok(value),
    }
}
