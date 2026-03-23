pub mod api;
pub mod auth;
pub mod config;
pub mod diagnostics;
pub mod error;
pub mod paths;
pub mod release;
pub mod service;
pub mod status;
pub mod types;

pub use api::{ApiClient, ApiTrafficObserver};
pub use auth::{WorkerAuthConfig, WorkerAuthenticator};
pub use config::{
    load_workers_config, resolve_workers_config_path, save_workers_config, AccelerationMode,
    EmbeddingWorkerSettings, RssWorkerSettings, ServiceMode, WorkersConfig, DEFAULT_API_URL,
    DEFAULT_EMBEDDING_INFERENCE_BATCH_SIZE, DEFAULT_EMBEDDING_LEASE_SECONDS,
    DEFAULT_EMBEDDING_POLL_SECONDS, DEFAULT_EMBEDDING_WORKER_VERSION,
    DEFAULT_RSS_FETCH_RETRY_COUNT, DEFAULT_RSS_HOST_MAX_REQUESTS_PER_SECOND,
    DEFAULT_RSS_LEASE_SECONDS, DEFAULT_RSS_MAX_CLAIMED_TASKS, DEFAULT_RSS_MAX_IN_FLIGHT_REQUESTS,
    DEFAULT_RSS_MAX_IN_FLIGHT_REQUESTS_PER_HOST, DEFAULT_RSS_POLL_SECONDS,
    DEFAULT_RSS_REQUEST_TIMEOUT_SECONDS,
};
pub use diagnostics::{check_worker_connection, WorkerConnectionCheck};
pub use error::{Result, WorkerError};
pub use paths::{app_paths, AppPaths, WorkerRuntimePaths};
pub use release::{
    check_worker_release_status, resolve_release_arch, resolve_release_platform,
    ReleaseCheckStatus, WorkerReleaseManifest, WorkerReleaseStatus,
};
pub use service::{
    install_user_service, start_user_service, stop_user_service, uninstall_user_service,
};
pub use status::{
    CurrentTaskSnapshot, NetworkTotalsSnapshot, ServerConnectionState, WorkerPhase,
    WorkerStatusHandle, WorkerStatusInit, WorkerStatusSnapshot,
};
pub use types::{WorkerTaskClaim, WorkerTaskClaimRequest, WorkerTaskCommand, WorkerType};
