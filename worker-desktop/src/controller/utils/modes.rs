use manifeed_worker_common::{
    AccelerationMode, EmbeddingRuntimeBundle, ServiceMode, WorkersConfig, DEFAULT_API_URL,
};

use crate::gpu::GpuSupport;
use crate::installer::resolved_runtime_bundle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServiceSyncAction {
    InstallBackgroundService,
    RemoveBackgroundService,
}

pub(crate) fn normalize_api_url(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_API_URL.to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

pub(crate) fn service_mode_index(mode: ServiceMode) -> i32 {
    match mode {
        ServiceMode::Manual => 0,
        ServiceMode::Background => 1,
    }
}

pub(crate) fn service_mode_from_index(index: i32) -> ServiceMode {
    match index {
        1 => ServiceMode::Background,
        _ => ServiceMode::Manual,
    }
}

pub(crate) fn acceleration_mode_index(mode: AccelerationMode) -> i32 {
    match mode {
        AccelerationMode::Auto => 0,
        AccelerationMode::Cpu => 1,
        AccelerationMode::Gpu => 2,
    }
}

pub(crate) fn acceleration_mode_from_index(index: i32) -> AccelerationMode {
    match index {
        1 => AccelerationMode::Cpu,
        2 => AccelerationMode::Gpu,
        _ => AccelerationMode::Auto,
    }
}

pub(crate) fn planned_service_sync(
    previous: ServiceMode,
    next: ServiceMode,
    installed: bool,
) -> Option<ServiceSyncAction> {
    if !installed || previous == next {
        return None;
    }

    match next {
        ServiceMode::Manual => Some(ServiceSyncAction::RemoveBackgroundService),
        ServiceMode::Background => Some(ServiceSyncAction::InstallBackgroundService),
    }
}

pub(crate) fn predicted_gpu_support(config: &WorkersConfig) -> GpuSupport {
    if cfg!(target_os = "macos") {
        return GpuSupport {
            recommended_backend: Some("coreml".to_string()),
            recommended_runtime_bundle: Some("coreml".to_string()),
            available_execution_providers: vec!["coreml".to_string()],
            notes: vec!["CoreML will be used after the embedding bundle is installed.".to_string()],
            error: None,
            runtime_load_error: None,
        };
    }

    match resolved_runtime_bundle(config) {
        Ok(EmbeddingRuntimeBundle::Cuda12) => GpuSupport {
            recommended_backend: Some("cuda".to_string()),
            recommended_runtime_bundle: Some("cuda12".to_string()),
            available_execution_providers: vec!["cuda".to_string()],
            notes: vec!["NVIDIA support detected. The CUDA12 bundle will be selected.".to_string()],
            error: None,
            runtime_load_error: None,
        },
        Ok(EmbeddingRuntimeBundle::CoreMl) => GpuSupport {
            recommended_backend: Some("coreml".to_string()),
            recommended_runtime_bundle: Some("coreml".to_string()),
            available_execution_providers: vec!["coreml".to_string()],
            notes: vec!["CoreML will be used after the embedding bundle is installed.".to_string()],
            error: None,
            runtime_load_error: None,
        },
        Ok(EmbeddingRuntimeBundle::None) | Ok(EmbeddingRuntimeBundle::WebGpu) => GpuSupport {
            notes: vec![
                "No compatible GPU bundle was detected. The CPU bundle will be used.".to_string(),
            ],
            ..GpuSupport::default()
        },
        Err(error) => GpuSupport {
            error: Some(error),
            ..GpuSupport::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use manifeed_worker_common::ServiceMode;

    use super::{planned_service_sync, ServiceSyncAction};

    #[test]
    fn service_sync_is_only_required_for_installed_workers() {
        assert_eq!(
            planned_service_sync(ServiceMode::Manual, ServiceMode::Background, false),
            None
        );
        assert_eq!(
            planned_service_sync(ServiceMode::Manual, ServiceMode::Background, true),
            Some(ServiceSyncAction::InstallBackgroundService)
        );
        assert_eq!(
            planned_service_sync(ServiceMode::Background, ServiceMode::Manual, true),
            Some(ServiceSyncAction::RemoveBackgroundService)
        );
    }
}
