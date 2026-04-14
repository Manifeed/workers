use std::time::Duration;

use manifeed_worker_common::{WorkerStatusHandle, WorkerStatusInit, WorkerType};
use tracing::{error, info, warn};
use worker_source_embedding::api::HttpEmbeddingGateway;
use worker_source_embedding::config::{
    EmbeddingWorkerConfig, EmbeddingWorkerConfigOverrides, FIXED_EMBEDDING_MODEL_NAME,
};
use worker_source_embedding::huggingface::HuggingFaceOnnxModelManager;
use worker_source_embedding::runtime::{probe_system, verify_execution_backend_support};
use worker_source_embedding::worker::EmbeddingWorker;

use crate::cli::RunArgs;

use super::shared::{
    acceleration_mode_label, map_acceleration_arg, map_provider_arg, validate_release_status,
    APP_VERSION, RUN_ERROR_SLEEP_SECONDS,
};

pub(crate) async fn run_command(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config = EmbeddingWorkerConfig::load(EmbeddingWorkerConfigOverrides {
        config_path: args.config,
        api_key: args.api_key,
        acceleration_mode: args.acceleration.map(map_acceleration_arg),
        provider_override: args.provider.map(map_provider_arg),
    })?;
    validate_release_status(&config.api_url)?;

    let ort_runtime_path = verify_execution_backend_support(
        config.execution_backend,
        config.ort_dylib_path.as_deref(),
    )?;
    let probe = probe_system(config.execution_backend, config.ort_dylib_path.as_deref());
    let status = WorkerStatusHandle::new(
        config.status_file_path.clone(),
        WorkerStatusInit {
            worker_type: WorkerType::SourceEmbedding,
            app_version: APP_VERSION.to_string(),
            acceleration_mode: Some(acceleration_mode_label(config.acceleration_mode).to_string()),
            execution_backend: Some(config.execution_backend.to_string()),
        },
    )?;

    let gateway = HttpEmbeddingGateway::new(&config, status.clone())?;
    let embedder = HuggingFaceOnnxModelManager::new(&config, status.clone())?;
    let mut worker = EmbeddingWorker::new(
        gateway,
        embedder,
        config.inference_batch_size,
        status.clone(),
    );

    info!(
        api_url = %config.api_url,
        config_path = %config.config_path.display(),
        worker_version = %config.worker_version,
        embedding_model_name = FIXED_EMBEDDING_MODEL_NAME,
        acceleration_mode = %acceleration_mode_label(config.acceleration_mode),
        execution_backend = %config.execution_backend,
        recommended_execution_backend = %probe.recommended_backend,
        recommended_runtime_bundle = %probe.recommended_runtime_bundle,
        ort_runtime_path = %ort_runtime_path.display(),
        status_file_path = %config.status_file_path.display(),
        model_cache_dir = %config.model_cache_dir.display(),
        "worker_source_embedding rust started"
    );
    if !probe.notes.is_empty() {
        warn!(notes = %probe.notes.join(" | "), "runtime probe warnings");
    }

    loop {
        match worker.run_once().await {
            Ok(processed) => {
                if !processed {
                    tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
                }
            }
            Err(error) if error.is_auth_error() => {
                let label = error.user_facing_message();
                let _ = status.mark_error(label.clone());
                error!(
                    "worker_source_embedding fatal authentication error: {}",
                    error
                );
                return Err(Box::new(error));
            }
            Err(error) if error.is_network_error() => {
                warn!(
                    retry_delay_seconds = config.poll_seconds,
                    "network error in embedding worker loop, retrying: {error}"
                );
                tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
            }
            Err(error) => {
                let _ = status.mark_error(error.user_facing_message());
                error!("worker_source_embedding iteration failed: {}", error);
                tokio::time::sleep(Duration::from_secs(RUN_ERROR_SLEEP_SECONDS)).await;
            }
        }
    }
}
