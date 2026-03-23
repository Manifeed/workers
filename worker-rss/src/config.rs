use std::env;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

use manifeed_worker_common::{
    app_paths, load_workers_config, WorkerAuthConfig, WorkerError, WorkerType, DEFAULT_API_URL,
};

use crate::error::Result;

#[derive(Clone, Debug, Default)]
pub struct RssWorkerConfigOverrides {
    pub config_path: Option<PathBuf>,
    pub api_url: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RssWorkerConfig {
    pub api_url: String,
    pub poll_seconds: u64,
    pub lease_seconds: u32,
    pub host_max_requests_per_second: u32,
    pub max_in_flight_requests: usize,
    pub max_in_flight_requests_per_host: usize,
    pub max_claimed_tasks: usize,
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

        let api_url = overrides
            .api_url
            .or_else(|| optional_env_string("MANIFEED_API_URL"))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if stored.rss.api_url.trim().is_empty() {
                    DEFAULT_API_URL.to_string()
                } else {
                    stored.rss.api_url.clone()
                }
            });
        let api_key = overrides
            .api_key
            .or_else(|| optional_env_string("MANIFEED_WORKER_API_KEY"))
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                (!stored.rss.api_key.trim().is_empty()).then(|| stored.rss.api_key.clone())
            })
            .ok_or_else(|| {
                WorkerError::Config(
                    "missing worker API key; run `worker-rss install --api-url ... --api-key ...` or set MANIFEED_WORKER_API_KEY"
                        .to_string(),
                )
            })?;

        Ok(Self {
            api_url,
            poll_seconds: env_or_value("MANIFEED_RSS_POLL_SECONDS", stored.rss.poll_seconds)?,
            lease_seconds: env_or_value("MANIFEED_RSS_LEASE_SECONDS", stored.rss.lease_seconds)?,
            host_max_requests_per_second: env_or_value(
                "MANIFEED_RSS_HOST_MAX_REQUESTS_PER_SECOND",
                stored.rss.host_max_requests_per_second,
            )?,
            max_in_flight_requests: env_or_value(
                "MANIFEED_RSS_MAX_IN_FLIGHT_REQUESTS",
                stored.rss.max_in_flight_requests,
            )?,
            max_in_flight_requests_per_host: env_or_value(
                "MANIFEED_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST",
                stored.rss.max_in_flight_requests_per_host,
            )?,
            max_claimed_tasks: env_or_value(
                "MANIFEED_RSS_MAX_CLAIMED_TASKS",
                stored.rss.max_claimed_tasks,
            )?,
            request_timeout_seconds: env_or_value(
                "MANIFEED_RSS_REQUEST_TIMEOUT_SECONDS",
                stored.rss.request_timeout_seconds,
            )?,
            fetch_retry_count: env_or_value(
                "MANIFEED_RSS_FETCH_RETRY_COUNT",
                stored.rss.fetch_retry_count,
            )?,
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
