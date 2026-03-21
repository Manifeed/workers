use std::time::Duration;

use tracing::{error, info, warn};
use worker_source_embedding::api::HttpEmbeddingGateway;
use worker_source_embedding::config::EmbeddingWorkerConfig;
use worker_source_embedding::huggingface::HuggingFaceOnnxModelManager;
use worker_source_embedding::runtime::{ensure_ort_runtime_loaded, probe_system};
use worker_source_embedding::status::WorkerStatusHandle;
use worker_source_embedding::worker::EmbeddingWorker;

const RUN_ERROR_SLEEP_SECONDS: u64 = 3;

enum WorkerCommand {
    Run,
    Probe,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_target(false).init();

    let command = parse_command()?;
    let config = EmbeddingWorkerConfig::from_env()?;

    match command {
        WorkerCommand::Run => run_worker(config).await,
        WorkerCommand::Probe => {
            let probe = probe_system(config.execution_backend, config.ort_dylib_path.as_deref());
            println!("{}", serde_json::to_string_pretty(&probe)?);
            Ok(())
        }
    }
}

async fn run_worker(config: EmbeddingWorkerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let probe = probe_system(config.execution_backend, config.ort_dylib_path.as_deref());
    let ort_runtime_path = ensure_ort_runtime_loaded(config.ort_dylib_path.as_deref())?;
    let status = WorkerStatusHandle::new(&config.status_file_path, config.execution_backend)?;

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
        worker_name = %config.auth.worker_name,
        execution_backend = %config.execution_backend,
        recommended_execution_backend = %probe.recommended_backend,
        recommended_runtime_bundle = %probe.recommended_runtime_bundle,
        ort_runtime_path = %ort_runtime_path.display(),
        status_file_path = %status.path().display(),
        model_cache_dir = %config.model_cache_dir.display(),
        huggingface_base_url = %config.huggingface_base_url,
        huggingface_default_revision = %config.huggingface_default_revision,
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
            Err(error) if error.is_network_error() => {
                warn!(
                    retry_delay_seconds = config.poll_seconds,
                    "network error in embedding worker loop, retrying: {error}"
                );
                tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
            }
            Err(error) => {
                let _ = status.mark_error(error.to_string());
                error!("worker_source_embedding iteration failed: {}", error);
                tokio::time::sleep(Duration::from_secs(RUN_ERROR_SLEEP_SECONDS)).await;
            }
        }
    }
}

fn parse_command() -> Result<WorkerCommand, Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None | Some("run") => Ok(WorkerCommand::Run),
        Some("probe") => Ok(WorkerCommand::Probe),
        Some("--help") | Some("-h") | Some("help") => {
            println!("usage: worker-source-embedding [run|probe]");
            std::process::exit(0);
        }
        Some(other) => Err(format!("unknown command: {other}").into()),
    }
}
