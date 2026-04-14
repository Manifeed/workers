use std::path::Path;

use manifeed_worker_common::{
    app_paths, check_worker_release_status, load_workers_config, AccelerationMode,
    EmbeddingRuntimeBundle, ReleaseCheckStatus, ServiceMode, WorkerType, DEFAULT_API_URL,
};
use tracing::warn;
use worker_source_embedding::runtime::ExecutionBackend;

use crate::cli::{AccelerationArg, ProviderArg};

pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const RUN_ERROR_SLEEP_SECONDS: u64 = 3;

pub(crate) fn validate_release_status(api_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let release = check_worker_release_status(
        api_url,
        WorkerType::SourceEmbedding.cli_product(),
        APP_VERSION,
        &app_paths()?.version_cache_dir().join(format!(
            "{}.json",
            WorkerType::SourceEmbedding.cli_product()
        )),
    )?;
    match release.status {
        ReleaseCheckStatus::Incompatible => {
            return Err(release
                .message
                .unwrap_or_else(|| "worker version is no longer supported".to_string())
                .into());
        }
        ReleaseCheckStatus::UpdateAvailable | ReleaseCheckStatus::Unverified => {
            if let Some(message) = release.message {
                warn!("{message}");
            }
        }
        ReleaseCheckStatus::UpToDate => {}
    }
    Ok(())
}

pub(crate) fn resolve_probe_acceleration_mode(
    config_path: Option<&Path>,
    acceleration: Option<AccelerationArg>,
) -> Result<AccelerationMode, Box<dyn std::error::Error>> {
    if let Some(acceleration) = acceleration {
        return Ok(map_acceleration_arg(acceleration));
    }
    let (_, config) = load_workers_config(config_path)?;
    Ok(config.embedding.acceleration_mode)
}

pub(crate) fn parse_service_mode(value: &str) -> Result<ServiceMode, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "manual" => Ok(ServiceMode::Manual),
        "background" => Ok(ServiceMode::Background),
        other => Err(format!("unsupported service mode: {other}").into()),
    }
}

pub(crate) fn parse_acceleration_mode(
    value: &str,
) -> Result<AccelerationMode, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(AccelerationMode::Auto),
        "cpu" => Ok(AccelerationMode::Cpu),
        "gpu" => Ok(AccelerationMode::Gpu),
        other => Err(format!("unsupported acceleration mode: {other}").into()),
    }
}

pub(crate) fn parse_runtime_bundle(
    value: &str,
) -> Result<EmbeddingRuntimeBundle, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Ok(EmbeddingRuntimeBundle::None),
        "cuda12" => Ok(EmbeddingRuntimeBundle::Cuda12),
        "webgpu" => Ok(EmbeddingRuntimeBundle::WebGpu),
        "coreml" => Ok(EmbeddingRuntimeBundle::CoreMl),
        other => Err(format!("unsupported runtime bundle: {other}").into()),
    }
}

pub(crate) fn map_acceleration_arg(value: AccelerationArg) -> AccelerationMode {
    match value {
        AccelerationArg::Auto => AccelerationMode::Auto,
        AccelerationArg::Cpu => AccelerationMode::Cpu,
        AccelerationArg::Gpu => AccelerationMode::Gpu,
    }
}

pub(crate) fn map_provider_arg(value: ProviderArg) -> ExecutionBackend {
    match value {
        ProviderArg::Auto => ExecutionBackend::Auto,
        ProviderArg::Cpu => ExecutionBackend::Cpu,
        ProviderArg::Cuda => ExecutionBackend::Cuda,
        ProviderArg::Webgpu => ExecutionBackend::WebGpu,
        ProviderArg::Coreml => ExecutionBackend::CoreMl,
    }
}

pub(crate) fn acceleration_mode_label(mode: AccelerationMode) -> &'static str {
    match mode {
        AccelerationMode::Auto => "auto",
        AccelerationMode::Cpu => "cpu",
        AccelerationMode::Gpu => "gpu",
    }
}

pub(crate) fn redact_secret(value: &str) -> String {
    if value.len() <= 8 {
        return "********".to_string();
    }
    format!("{}***{}", &value[..4], &value[value.len() - 4..])
}

pub(crate) fn version_command_default_api_url() -> &'static str {
    DEFAULT_API_URL
}
