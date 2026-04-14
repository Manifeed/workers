use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ort::ep::{self, ExecutionProvider, ExecutionProviderDispatch};

use crate::error::{EmbeddingWorkerError, Result};

use super::{ort_dylib_candidates, ExecutionBackend, RuntimeProbe, RuntimeSupport};

static ORT_RUNTIME_PATH: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();

pub fn ensure_ort_runtime_loaded(explicit_ort_dylib_path: Option<&Path>) -> Result<PathBuf> {
    let loaded = ORT_RUNTIME_PATH.get_or_init(|| {
        let candidates = ort_dylib_candidates(explicit_ort_dylib_path);
        for candidate in &candidates {
            if !candidate.exists() {
                continue;
            }
            match ort::init_from(candidate) {
                Ok(builder) => {
                    let _ = builder.commit();
                    return Ok(candidate.clone());
                }
                Err(error) => {
                    return Err(format!(
                        "failed to load ONNX Runtime dynamic library {}: {error}",
                        candidate.display()
                    ));
                }
            }
        }

        Err(format!(
            "unable to locate {}; tried: {}",
            super::onnxruntime_dylib_name(),
            candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    });

    loaded.clone().map_err(EmbeddingWorkerError::Runtime)
}

pub fn verify_execution_backend_support(
    requested_backend: ExecutionBackend,
    explicit_ort_dylib_path: Option<&Path>,
) -> Result<PathBuf> {
    let runtime_support = probe_runtime_support(explicit_ort_dylib_path);
    let ort_runtime_path = runtime_support.ort_runtime_path.ok_or_else(|| {
        EmbeddingWorkerError::Runtime(
            runtime_support
                .runtime_load_error
                .unwrap_or_else(|| "unable to load ONNX Runtime".to_string()),
        )
    })?;

    if matches!(
        requested_backend,
        ExecutionBackend::Auto | ExecutionBackend::Cpu
    ) {
        return Ok(ort_runtime_path);
    }

    if runtime_support
        .available_execution_providers
        .contains(&requested_backend)
    {
        return Ok(ort_runtime_path);
    }

    Err(EmbeddingWorkerError::Runtime(format!(
        "{} execution provider is not enabled in this build; loaded runtime: {}; available execution providers: {}",
        requested_backend.as_str().to_ascii_uppercase(),
        ort_runtime_path.display(),
        format_execution_backends(&runtime_support.available_execution_providers),
    )))
}

pub fn execution_providers(
    preferred_backend: ExecutionBackend,
    probe: &RuntimeProbe,
) -> Vec<ExecutionProviderDispatch> {
    match preferred_backend {
        ExecutionBackend::Auto => match probe.recommended_backend {
            ExecutionBackend::Cuda => vec![
                ep::CUDA::default().build(),
                ep::WebGPU::default().build(),
                ep::CPU::default().build(),
            ],
            ExecutionBackend::WebGpu => vec![
                ep::WebGPU::default().build(),
                ep::CUDA::default().build(),
                ep::CPU::default().build(),
            ],
            ExecutionBackend::CoreMl => vec![
                ep::CoreML::default()
                    .with_compute_units(ep::coreml::ComputeUnits::CPUAndNeuralEngine)
                    .build(),
                ep::CPU::default().build(),
            ],
            _ => vec![ep::CPU::default().build()],
        },
        ExecutionBackend::Cpu => vec![ep::CPU::default().build().error_on_failure()],
        ExecutionBackend::Cuda => vec![
            ep::CUDA::default().build().error_on_failure(),
            ep::CPU::default().build(),
        ],
        ExecutionBackend::WebGpu => vec![
            ep::WebGPU::default().build().error_on_failure(),
            ep::CPU::default().build(),
        ],
        ExecutionBackend::CoreMl => vec![
            ep::CoreML::default()
                .with_compute_units(ep::coreml::ComputeUnits::CPUAndNeuralEngine)
                .build()
                .error_on_failure(),
            ep::CPU::default().build(),
        ],
    }
}

pub fn available_execution_providers() -> Vec<ExecutionBackend> {
    let mut providers = Vec::new();
    if ep::CUDA::default().is_available().unwrap_or(false) {
        providers.push(ExecutionBackend::Cuda);
    }
    if ep::WebGPU::default().is_available().unwrap_or(false) {
        providers.push(ExecutionBackend::WebGpu);
    }
    if ep::CoreML::default().is_available().unwrap_or(false) {
        providers.push(ExecutionBackend::CoreMl);
    }
    providers.push(ExecutionBackend::Cpu);
    providers
}

pub(crate) fn probe_runtime_support(explicit_ort_dylib_path: Option<&Path>) -> RuntimeSupport {
    match ensure_ort_runtime_loaded(explicit_ort_dylib_path) {
        Ok(ort_runtime_path) => RuntimeSupport {
            ort_runtime_path: Some(ort_runtime_path),
            available_execution_providers: available_execution_providers(),
            runtime_load_error: None,
        },
        Err(error) => RuntimeSupport {
            ort_runtime_path: None,
            available_execution_providers: Vec::new(),
            runtime_load_error: Some(error.to_string()),
        },
    }
}

pub(crate) fn format_execution_backends(backends: &[ExecutionBackend]) -> String {
    if backends.is_empty() {
        return "none".to_string();
    }

    backends
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
