use std::path::Path;
use std::process::Command;

use manifeed_worker_common::{AccelerationMode, EmbeddingRuntimeBundle, WorkerType, WorkersConfig};

pub(crate) fn resolved_runtime_bundle(
    config: &WorkersConfig,
) -> Result<EmbeddingRuntimeBundle, String> {
    if std::env::consts::OS == "macos" {
        return Ok(match config.embedding.acceleration_mode {
            AccelerationMode::Cpu => EmbeddingRuntimeBundle::None,
            AccelerationMode::Auto | AccelerationMode::Gpu => EmbeddingRuntimeBundle::CoreMl,
        });
    }

    if std::env::consts::OS != "linux" {
        return Ok(EmbeddingRuntimeBundle::None);
    }

    match config.embedding.acceleration_mode {
        AccelerationMode::Cpu => Ok(EmbeddingRuntimeBundle::None),
        AccelerationMode::Auto => {
            if std::env::consts::ARCH == "x86_64" && has_nvidia_support() {
                return Ok(EmbeddingRuntimeBundle::Cuda12);
            }
            Ok(EmbeddingRuntimeBundle::None)
        }
        AccelerationMode::Gpu => {
            if std::env::consts::ARCH == "x86_64" && has_nvidia_support() {
                return Ok(EmbeddingRuntimeBundle::Cuda12);
            }
            Err(
                "GPU acceleration requested but no supported Linux bundle was detected. Use CPU or install on Apple Silicon for CoreML."
                    .to_string(),
            )
        }
    }
}

pub(crate) fn manifest_runtime_bundle(
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<Option<String>, String> {
    if worker_type != WorkerType::SourceEmbedding {
        return Ok(None);
    }

    Ok(Some(
        runtime_bundle_slug(resolved_runtime_bundle(config)?).to_string(),
    ))
}

pub(super) fn runtime_library_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

fn runtime_bundle_slug(bundle: EmbeddingRuntimeBundle) -> &'static str {
    match bundle {
        EmbeddingRuntimeBundle::None => "none",
        EmbeddingRuntimeBundle::Cuda12 => "cuda12",
        EmbeddingRuntimeBundle::WebGpu => "webgpu",
        EmbeddingRuntimeBundle::CoreMl => "coreml",
    }
}

fn has_nvidia_support() -> bool {
    command_exists("nvidia-smi")
        || Path::new("/usr/lib/x86_64-linux-gnu/libcuda.so.1").exists()
        || Path::new("/usr/lib64/libcuda.so.1").exists()
        || Path::new("/usr/local/cuda").exists()
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {command} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
