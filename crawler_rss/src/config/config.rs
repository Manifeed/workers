use std::env;
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::auth::WorkerAuthConfig;
use crate::error::Result;
use crate::error::WorkerError;
use crate::paths::app_paths;
use crate::types::WorkerType;

pub const DEFAULT_API_URL: &str = "https://api.manifeed.app";

const BUILTIN_RSS_POLL_SECONDS: u64 = 5;
const BUILTIN_RSS_LEASE_SECONDS: u32 = 120;
const BUILTIN_RSS_SESSION_TTL_SECONDS: u32 = 3600;
const BUILTIN_RSS_HOST_MAX_REQUESTS_PER_SECOND: u32 = 10;
const BUILTIN_RSS_MAX_IN_FLIGHT_REQUESTS: usize = 5;
const BUILTIN_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST: usize = 3;
const BUILTIN_RSS_REQUEST_TIMEOUT_SECONDS: u64 = 10;
const BUILTIN_RSS_FETCH_RETRY_COUNT: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct RssWorkerConfigOverrides {
    pub config_path: Option<PathBuf>,
    pub api_url: Option<String>,
    pub api_key: Option<String>,
    pub concurrency: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CrawlerRssConfigFile {
    pub schema_version: u32,
    pub api_url: String,
    pub api_key: String,
    pub concurrency: usize,
}

#[derive(Clone, Debug)]
pub struct RssWorkerConfig {
    pub api_url: String,
    pub session_ttl_seconds: u32,
    pub poll_seconds: u64,
    pub lease_seconds: u32,
    pub host_max_requests_per_second: u32,
    pub max_in_flight_requests: usize,
    pub max_in_flight_requests_per_host: usize,
    pub request_timeout_seconds: u64,
    pub fetch_retry_count: u32,
    pub config_path: PathBuf,
    pub auth: WorkerAuthConfig,
}

impl Default for CrawlerRssConfigFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            api_url: DEFAULT_API_URL.to_string(),
            api_key: String::new(),
            concurrency: BUILTIN_RSS_MAX_IN_FLIGHT_REQUESTS,
        }
    }
}

impl RssWorkerConfig {
    pub fn load(overrides: RssWorkerConfigOverrides) -> Result<Self> {
        let config_path = resolve_crawler_rss_config_path(overrides.config_path.as_deref())?;
        let stored = load_crawler_rss_config(Some(config_path.as_path()))?;

        let api_url = first_non_empty([
            overrides.api_url,
            optional_env_string("MANIFEED_CRAWLER_RSS_API_URL"),
            optional_env_string("MANIFEED_WORKER_API_URL"),
            optional_env_string("MANIFEED_API_URL"),
            Some(stored.api_url.clone()),
        ])
        .unwrap_or_else(|| DEFAULT_API_URL.to_string());

        let api_key = first_non_empty([
            overrides.api_key,
            optional_env_string("MANIFEED_CRAWLER_RSS_API_KEY"),
            optional_env_string("MANIFEED_WORKER_API_KEY"),
            Some(stored.api_key.clone()),
        ])
        .ok_or_else(|| {
            WorkerError::Config(
                "missing crawler RSS API key; set MANIFEED_CRAWLER_RSS_API_KEY or run `crawler_rss config set api-key ...`"
                    .to_string(),
            )
        })?;

        Ok(Self {
            api_url,
            session_ttl_seconds: env_or_value(
                "MANIFEED_CRAWLER_RSS_SESSION_TTL_SECONDS",
                BUILTIN_RSS_SESSION_TTL_SECONDS,
            )?,
            poll_seconds: env_or_value(
                "MANIFEED_CRAWLER_RSS_POLL_SECONDS",
                BUILTIN_RSS_POLL_SECONDS,
            )?,
            lease_seconds: env_or_value(
                "MANIFEED_CRAWLER_RSS_LEASE_SECONDS",
                BUILTIN_RSS_LEASE_SECONDS,
            )?,
            host_max_requests_per_second: env_or_value(
                "MANIFEED_CRAWLER_RSS_HOST_MAX_REQUESTS_PER_SECOND",
                BUILTIN_RSS_HOST_MAX_REQUESTS_PER_SECOND,
            )?,
            max_in_flight_requests: override_env_or_value(
                overrides.concurrency,
                "MANIFEED_CRAWLER_RSS_CONCURRENCY",
                stored.concurrency,
            )?
            .max(1),
            max_in_flight_requests_per_host: env_or_value(
                "MANIFEED_CRAWLER_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST",
                BUILTIN_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST,
            )?
            .max(1),
            request_timeout_seconds: env_or_value(
                "MANIFEED_CRAWLER_RSS_REQUEST_TIMEOUT_SECONDS",
                BUILTIN_RSS_REQUEST_TIMEOUT_SECONDS,
            )?
            .max(1),
            fetch_retry_count: env_or_value(
                "MANIFEED_CRAWLER_RSS_FETCH_RETRY_COUNT",
                BUILTIN_RSS_FETCH_RETRY_COUNT,
            )?,
            config_path,
            auth: WorkerAuthConfig {
                worker_type: WorkerType::RssScrapper,
                api_key,
            },
        })
    }
}

pub fn resolve_crawler_rss_config_path(explicit_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit_path {
        return Ok(path.to_path_buf());
    }
    Ok(app_paths()?.config_dir.join("crawler_rss.json"))
}

pub fn load_crawler_rss_config(explicit_path: Option<&Path>) -> Result<CrawlerRssConfigFile> {
    let path = resolve_crawler_rss_config_path(explicit_path)?;
    if !path.exists() {
        return Ok(CrawlerRssConfigFile::default());
    }
    let payload = fs::read(path)?;
    let mut config = serde_json::from_slice::<CrawlerRssConfigFile>(&payload)?;
    config.schema_version = 1;
    if config.api_url.trim().is_empty() {
        config.api_url = DEFAULT_API_URL.to_string();
    }
    Ok(config)
}

pub fn save_crawler_rss_config(path: &Path, config: &CrawlerRssConfigFile) -> Result<()> {
    let mut normalized = config.clone();
    normalized.schema_version = 1;
    normalized.concurrency = normalized.concurrency.max(1);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, serde_json::to_vec_pretty(&normalized)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temp_path, path)?;
    Ok(())
}

fn first_non_empty(values: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

fn optional_env_string(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn override_env_or_value<T>(override_value: Option<T>, key: &str, value: T) -> Result<T>
where
    T: Clone + FromStr,
    T::Err: Display,
{
    if let Some(value) = override_value {
        return Ok(value);
    }
    match optional_env_string(key) {
        Some(raw) => raw.parse::<T>().map_err(|error| {
            WorkerError::Config(format!("invalid value for {key}: {raw} ({error})")).into()
        }),
        None => Ok(value),
    }
}

fn env_or_value<T>(key: &str, value: T) -> Result<T>
where
    T: Clone + FromStr,
    T::Err: Display,
{
    override_env_or_value(None, key, value)
}
