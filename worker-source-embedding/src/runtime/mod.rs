use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::str::FromStr;

use serde::Serialize;

use crate::error::{EmbeddingWorkerError, Result};

mod bundle;
mod probe;
mod providers;
mod system;

pub use bundle::onnxruntime_dylib_name;
pub use probe::probe_system;
pub use providers::{
    available_execution_providers, ensure_ort_runtime_loaded, execution_providers,
    verify_execution_backend_support,
};

pub(crate) use bundle::ort_dylib_candidates;
pub(crate) use providers::{format_execution_backends, probe_runtime_support};
pub(crate) use system::{
    command_exists, detect_gpu_vendors, detect_render_node, has_shared_library, read_os_release,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionBackend {
    #[default]
    Auto,
    Cpu,
    Cuda,
    WebGpu,
    CoreMl,
}

impl ExecutionBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
            Self::WebGpu => "webgpu",
            Self::CoreMl => "coreml",
        }
    }
}

impl Display for ExecutionBackend {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ExecutionBackend {
    type Err = EmbeddingWorkerError;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "cpu" => Ok(Self::Cpu),
            "cuda" => Ok(Self::Cuda),
            "webgpu" | "wgpu" => Ok(Self::WebGpu),
            "coreml" => Ok(Self::CoreMl),
            other => Err(EmbeddingWorkerError::Runtime(format!(
                "unsupported execution backend: {other}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBundle {
    None,
    Cuda12,
    WebGpu,
    CoreMl,
}

impl RuntimeBundle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Cuda12 => "cuda12",
            Self::WebGpu => "webgpu",
            Self::CoreMl => "coreml",
        }
    }
}

impl Display for RuntimeBundle {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeProbe {
    pub os: String,
    pub arch: String,
    pub distro_id: Option<String>,
    pub distro_name: Option<String>,
    pub distro_version: Option<String>,
    pub gpu_vendors: Vec<GpuVendor>,
    pub has_render_node: bool,
    pub has_nvidia_smi: bool,
    pub has_cuda_driver: bool,
    pub has_vulkan_loader: bool,
    pub ort_runtime_path: Option<String>,
    pub available_execution_providers: Vec<ExecutionBackend>,
    pub runtime_load_error: Option<String>,
    pub recommended_backend: ExecutionBackend,
    pub recommended_runtime_bundle: RuntimeBundle,
    pub ort_dylib_candidates: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeSupport {
    pub ort_runtime_path: Option<PathBuf>,
    pub available_execution_providers: Vec<ExecutionBackend>,
    pub runtime_load_error: Option<String>,
}
