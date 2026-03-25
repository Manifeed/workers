use std::env;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

use manifeed_worker_common::{
    app_paths, load_workers_config, WorkerAuthConfig, WorkerError, WorkerType, DEFAULT_API_URL,
};

use crate::error::Result;

const BUILTIN_RSS_POLL_SECONDS: u64 = 5;
const BUILTIN_RSS_LEASE_SECONDS: u32 = 120;
const BUILTIN_RSS_HOST_MAX_REQUESTS_PER_SECOND: u32 = 20;
const BUILTIN_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST: usize = 4;
const BUILTIN_RSS_REQUEST_TIMEOUT_SECONDS: u64 = 10;
const BUILTIN_RSS_FETCH_RETRY_COUNT: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct RssWorkerConfigOverrides {
    pub config_path: Option<PathBuf>,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RssWorkerConfig {
    pub api_url: String,
    pub worker_class: String,
    pub queue_lane: String,
    pub session_ttl_seconds: u32,
    pub poll_seconds: u64,
    pub lease_seconds: u32,
    pub host_max_requests_per_second: u32,
    pub max_in_flight_requests: usize,
    pub max_in_flight_requests_per_host: usize,
    pub request_timeout_seconds: u64,
    pub fetch_retry_count: u32,
    pub status_file_path: PathBuf,
    pub version_cache_path: PathBuf,
    pub config_path: PathBuf,
    pub auth: WorkerAuthConfig,
}

impl RssWorkerConfig {
    pub fn load(overrides: RssWorkerConfigOverrides) -> Result<Self> {
        let app_dirs = app_paths()?;
        let worker_paths = app_dirs.worker_paths(WorkerType::RssScrapper);
        let (config_path, stored) = load_workers_config(overrides.config_path.as_deref())?;

        let api_key = overrides
            .api_key
            .or_else(|| optional_env_string("MANIFEED_WORKER_API_KEY"))
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                (!stored.rss.api_key.trim().is_empty()).then(|| stored.rss.api_key.clone())
            })
            .ok_or_else(|| {
                WorkerError::Config(
                    "missing worker API key; run `worker-rss install --api-key ...` or set MANIFEED_WORKER_API_KEY"
                        .to_string(),
                )
            })?;

        Ok(Self {
            api_url: DEFAULT_API_URL.to_string(),
            worker_class: optional_env_string("MANIFEED_RSS_WORKER_CLASS")
                .unwrap_or_else(|| "external".to_string()),
            queue_lane: optional_env_string("MANIFEED_RSS_QUEUE_LANE")
                .unwrap_or_else(|| "safe".to_string()),
            session_ttl_seconds: env_or_value("MANIFEED_RSS_SESSION_TTL_SECONDS", 3600u32)?,
            poll_seconds: BUILTIN_RSS_POLL_SECONDS,
            lease_seconds: BUILTIN_RSS_LEASE_SECONDS,
            host_max_requests_per_second: BUILTIN_RSS_HOST_MAX_REQUESTS_PER_SECOND,
            max_in_flight_requests: stored.rss.max_in_flight_requests.max(1),
            max_in_flight_requests_per_host: BUILTIN_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST,
            request_timeout_seconds: BUILTIN_RSS_REQUEST_TIMEOUT_SECONDS,
            fetch_retry_count: BUILTIN_RSS_FETCH_RETRY_COUNT,
            status_file_path: worker_paths.status_file,
            version_cache_path: app_dirs
                .version_cache_dir()
                .join(format!("{}.json", WorkerType::RssScrapper.cli_product())),
            config_path,
            auth: WorkerAuthConfig {
                worker_type: WorkerType::RssScrapper,
                api_key,
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
