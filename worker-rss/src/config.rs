use std::env;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

use manifeed_worker_common::{WorkerAuthConfig, WorkerError, WorkerType};

use crate::error::Result;

const DEFAULT_API_URL: &str = "http://127.0.0.1:8000";
const DEFAULT_POLL_SECONDS: u64 = 5;
const DEFAULT_LEASE_SECONDS: u32 = 300;
const DEFAULT_HOST_MAX_REQUESTS_PER_SECOND: u32 = 20;
const DEFAULT_MAX_IN_FLIGHT_REQUESTS: usize = 5;
const DEFAULT_MAX_IN_FLIGHT_REQUESTS_PER_HOST: usize = 5;
const DEFAULT_MAX_CLAIMED_TASKS: usize = 5;
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 10;
const DEFAULT_FETCH_RETRY_COUNT: u32 = 1;

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
    pub auth: WorkerAuthConfig,
}

impl RssWorkerConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            api_url: optional_env_string("MANIFEED_API_URL")
                .unwrap_or_else(|| DEFAULT_API_URL.to_string()),
            poll_seconds: env_or_default("MANIFEED_RSS_POLL_SECONDS", DEFAULT_POLL_SECONDS)?,
            lease_seconds: env_or_default("MANIFEED_RSS_LEASE_SECONDS", DEFAULT_LEASE_SECONDS)?,
            host_max_requests_per_second: env_or_default(
                "MANIFEED_RSS_HOST_MAX_REQUESTS_PER_SECOND",
                DEFAULT_HOST_MAX_REQUESTS_PER_SECOND,
            )?,
            max_in_flight_requests: env_or_default(
                "MANIFEED_RSS_MAX_IN_FLIGHT_REQUESTS",
                DEFAULT_MAX_IN_FLIGHT_REQUESTS,
            )?,
            max_in_flight_requests_per_host: env_or_default(
                "MANIFEED_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST",
                DEFAULT_MAX_IN_FLIGHT_REQUESTS_PER_HOST,
            )?,
            max_claimed_tasks: env_or_default(
                "MANIFEED_RSS_MAX_CLAIMED_TASKS",
                DEFAULT_MAX_CLAIMED_TASKS,
            )?,
            request_timeout_seconds: env_or_default(
                "MANIFEED_RSS_REQUEST_TIMEOUT_SECONDS",
                DEFAULT_REQUEST_TIMEOUT_SECONDS,
            )?,
            fetch_retry_count: env_or_default(
                "MANIFEED_RSS_FETCH_RETRY_COUNT",
                DEFAULT_FETCH_RETRY_COUNT,
            )?,
            auth: WorkerAuthConfig {
                worker_type: WorkerType::RssScrapper,
                identity_dir: optional_env_path("MANIFEED_RSS_IDENTITY_DIR"),
                enrollment_token: optional_env_string("MANIFEED_RSS_ENROLLMENT_TOKEN"),
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
