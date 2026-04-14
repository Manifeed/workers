use manifeed_worker_common::{
    app_paths, check_worker_connection, check_worker_release_status, load_workers_config,
    WorkerType,
};
use serde_json::json;
use worker_source_embedding::config::{EmbeddingWorkerConfig, EmbeddingWorkerConfigOverrides};
use worker_source_embedding::runtime::probe_system;

use crate::cli::CommonConfigArgs;

use super::shared::APP_VERSION;

pub(crate) fn doctor_command(args: CommonConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config = EmbeddingWorkerConfig::load(EmbeddingWorkerConfigOverrides {
        config_path: args.config.clone(),
        api_key: None,
        acceleration_mode: None,
        provider_override: None,
    })?;
    let app_dirs = app_paths()?;
    let worker_paths = app_dirs.worker_paths(WorkerType::SourceEmbedding);
    let (_, stored_config) = load_workers_config(Some(config.config_path.as_path()))?;
    let connection = check_worker_connection(&config.api_url, config.auth.api_key.as_str()).ok();
    let release = check_worker_release_status(
        &config.api_url,
        WorkerType::SourceEmbedding.cli_product(),
        APP_VERSION,
        config.version_cache_path.as_path(),
    )?;
    let probe = probe_system(config.execution_backend, config.ort_dylib_path.as_deref());
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "worker_type": WorkerType::SourceEmbedding.as_str(),
            "app_version": APP_VERSION,
            "config_path": config.config_path,
            "api_url": config.api_url,
            "status_file": worker_paths.status_file,
            "log_file": worker_paths.log_file,
            "model_cache_dir": worker_paths.cache_dir,
            "installed_version": stored_config.embedding.installed_version,
            "acceleration_mode": config.acceleration_mode,
            "runtime_bundle": config.runtime_bundle,
            "execution_backend": config.execution_backend,
            "connection": connection,
            "release": release,
            "probe": probe,
        }))?
    );
    Ok(())
}
