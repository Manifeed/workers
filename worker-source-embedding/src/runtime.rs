use std::env;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::OnceLock;

use ort::ep::{self, ExecutionProvider, ExecutionProviderDispatch};
use serde::Serialize;

use crate::error::{EmbeddingWorkerError, Result};

static ORT_RUNTIME_PATH: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();

pub fn onnxruntime_dylib_name() -> &'static str {
    match env::consts::OS {
        "macos" => "libonnxruntime.dylib",
        _ => "libonnxruntime.so",
    }
}

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
struct RuntimeSupport {
    ort_runtime_path: Option<PathBuf>,
    available_execution_providers: Vec<ExecutionBackend>,
    runtime_load_error: Option<String>,
}

pub fn probe_system(
    preferred_backend: ExecutionBackend,
    explicit_ort_dylib_path: Option<&Path>,
) -> RuntimeProbe {
    let os = env::consts::OS.to_string();
    let arch = env::consts::ARCH.to_string();
    let (distro_id, distro_name, distro_version) = if os == "linux" {
        read_os_release()
    } else {
        (None, None, None)
    };

    let gpu_vendors = if os == "linux" {
        detect_gpu_vendors()
    } else {
        Vec::new()
    };
    let has_render_node = os == "linux" && detect_render_node();
    let has_nvidia_smi = os == "linux" && command_exists("nvidia-smi");
    let has_cuda_driver =
        os == "linux" && (has_shared_library("libcuda.so") || has_shared_library("libcuda.so.1"));
    let has_vulkan_loader = os == "linux"
        && (has_shared_library("libvulkan.so") || has_shared_library("libvulkan.so.1"));

    let mut notes = Vec::new();
    if os == "linux" && arch != "x86_64" {
        notes.push(
            "current installer only ships GPU ONNX Runtime bundles for linux x86_64; CPU runtime will be used"
                .to_string(),
        );
    }
    if os == "macos" && arch == "aarch64" {
        notes.push(
            "Apple Silicon detected; CoreML with CPUAndNeuralEngine is the recommended acceleration backend"
                .to_string(),
        );
    }
    if os == "linux" && gpu_vendors.contains(&GpuVendor::Nvidia) && !has_cuda_driver {
        notes.push(
            "nvidia GPU detected but CUDA driver/runtime was not found; CUDA bundle is not recommended"
                .to_string(),
        );
    }
    if os == "linux" && !gpu_vendors.is_empty() && !has_vulkan_loader {
        notes.push(
            "GPU detected but Vulkan loader was not found; WebGPU bundle is not recommended"
                .to_string(),
        );
    }
    if os == "linux"
        && arch == "x86_64"
        && gpu_vendors
            .iter()
            .any(|vendor| matches!(vendor, GpuVendor::Amd | GpuVendor::Intel | GpuVendor::Other))
    {
        notes.push(
            "automatic Linux installer currently provisions official ONNX Runtime CPU/CUDA bundles only; non-NVIDIA GPUs fall back to CPU unless you provide a custom WebGPU-enabled ONNX Runtime"
                .to_string(),
        );
    }

    let ort_dylib_candidates = ort_dylib_candidates(explicit_ort_dylib_path)
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let runtime_support = probe_runtime_support(explicit_ort_dylib_path);

    let (recommended_backend, recommended_runtime_bundle) = recommend_runtime(
        preferred_backend,
        &os,
        &arch,
        &gpu_vendors,
        has_cuda_driver,
        has_vulkan_loader,
    );
    if let Some(runtime_load_error) = runtime_support.runtime_load_error.as_ref() {
        notes.push(format!(
            "unable to inspect installed ONNX Runtime providers: {runtime_load_error}"
        ));
    } else if matches!(
        recommended_backend,
        ExecutionBackend::Cuda | ExecutionBackend::WebGpu | ExecutionBackend::CoreMl
    ) && !runtime_support
        .available_execution_providers
        .contains(&recommended_backend)
    {
        notes.push(format!(
            "GPU hardware was detected, but the installed ONNX Runtime currently exposes {}; switch acceleration to auto/cpu or reinstall the worker runtime for {recommended_backend}",
            format_execution_backends(&runtime_support.available_execution_providers),
        ));
    }

    RuntimeProbe {
        os,
        arch,
        distro_id,
        distro_name,
        distro_version,
        gpu_vendors,
        has_render_node,
        has_nvidia_smi,
        has_cuda_driver,
        has_vulkan_loader,
        ort_runtime_path: runtime_support
            .ort_runtime_path
            .as_ref()
            .map(|path| path.display().to_string()),
        available_execution_providers: runtime_support.available_execution_providers,
        runtime_load_error: runtime_support.runtime_load_error,
        recommended_backend,
        recommended_runtime_bundle,
        ort_dylib_candidates,
        notes,
    }
}

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
            onnxruntime_dylib_name(),
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

fn probe_runtime_support(explicit_ort_dylib_path: Option<&Path>) -> RuntimeSupport {
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

fn format_execution_backends(backends: &[ExecutionBackend]) -> String {
    if backends.is_empty() {
        return "none".to_string();
    }

    backends
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn recommend_runtime(
    preferred_backend: ExecutionBackend,
    os: &str,
    arch: &str,
    gpu_vendors: &[GpuVendor],
    has_cuda_driver: bool,
    _has_vulkan_loader: bool,
) -> (ExecutionBackend, RuntimeBundle) {
    match preferred_backend {
        ExecutionBackend::Cpu => (ExecutionBackend::Cpu, RuntimeBundle::None),
        ExecutionBackend::Cuda => {
            if os == "linux" && arch == "x86_64" {
                (ExecutionBackend::Cuda, RuntimeBundle::Cuda12)
            } else {
                (ExecutionBackend::Cpu, RuntimeBundle::None)
            }
        }
        ExecutionBackend::WebGpu => {
            if os == "linux" && arch == "x86_64" {
                (ExecutionBackend::WebGpu, RuntimeBundle::WebGpu)
            } else {
                (ExecutionBackend::Cpu, RuntimeBundle::None)
            }
        }
        ExecutionBackend::CoreMl => {
            if os == "macos" && arch == "aarch64" {
                (ExecutionBackend::CoreMl, RuntimeBundle::CoreMl)
            } else {
                (ExecutionBackend::Cpu, RuntimeBundle::None)
            }
        }
        ExecutionBackend::Auto => {
            if os == "macos" && arch == "aarch64" {
                return (ExecutionBackend::CoreMl, RuntimeBundle::CoreMl);
            }
            if os == "linux"
                && arch == "x86_64"
                && gpu_vendors.contains(&GpuVendor::Nvidia)
                && has_cuda_driver
            {
                return (ExecutionBackend::Cuda, RuntimeBundle::Cuda12);
            }
            (ExecutionBackend::Cpu, RuntimeBundle::None)
        }
    }
}

fn ort_dylib_candidates(explicit_ort_dylib_path: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit_ort_dylib_path {
        candidates.push(path.to_path_buf());
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join("lib").join(onnxruntime_dylib_name()));
            candidates.push(parent.join(onnxruntime_dylib_name()));
        }
    }

    for candidate in onnxruntime_system_candidates() {
        candidates.push(PathBuf::from(candidate));
    }

    dedupe_paths(candidates)
}

fn onnxruntime_system_candidates() -> &'static [&'static str] {
    match env::consts::OS {
        "macos" => &[
            "/opt/homebrew/lib/libonnxruntime.dylib",
            "/usr/local/lib/libonnxruntime.dylib",
            "/usr/lib/libonnxruntime.dylib",
        ],
        _ => &[
            "/usr/lib/manifeed/embedding/runtime/lib/libonnxruntime.so",
            "/usr/lib/libonnxruntime.so",
            "/usr/lib64/libonnxruntime.so",
            "/usr/local/lib/libonnxruntime.so",
            "/usr/local/lib64/libonnxruntime.so",
            "/lib/x86_64-linux-gnu/libonnxruntime.so",
            "/lib/aarch64-linux-gnu/libonnxruntime.so",
        ],
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if unique.iter().any(|existing| existing == &path) {
            continue;
        }
        unique.push(path);
    }
    unique
}

fn read_os_release() -> (Option<String>, Option<String>, Option<String>) {
    let contents = match fs::read_to_string("/etc/os-release") {
        Ok(contents) => contents,
        Err(_) => return (None, None, None),
    };

    let mut distro_id = None;
    let mut distro_name = None;
    let mut distro_version = None;
    for line in contents.lines() {
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let value = raw_value.trim().trim_matches('"').to_string();
        match key.trim() {
            "ID" => distro_id = Some(value),
            "NAME" => distro_name = Some(value),
            "VERSION_ID" => distro_version = Some(value),
            _ => {}
        }
    }
    (distro_id, distro_name, distro_version)
}

fn detect_gpu_vendors() -> Vec<GpuVendor> {
    let mut vendors = Vec::new();
    let drm_dir = match fs::read_dir("/sys/class/drm") {
        Ok(entries) => entries,
        Err(_) => return vendors,
    };

    for entry in drm_dir.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with("card") || file_name.contains('-') {
            continue;
        }

        let vendor_path = entry.path().join("device/vendor");
        let Ok(raw_vendor) = fs::read_to_string(vendor_path) else {
            continue;
        };
        let vendor = match raw_vendor.trim() {
            "0x10de" => GpuVendor::Nvidia,
            "0x1002" => GpuVendor::Amd,
            "0x8086" => GpuVendor::Intel,
            _ => GpuVendor::Other,
        };
        if !vendors.contains(&vendor) {
            vendors.push(vendor);
        }
    }

    vendors
}

fn detect_render_node() -> bool {
    match fs::read_dir("/dev/dri") {
        Ok(entries) => entries
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD")),
        Err(_) => false,
    }
}

fn command_exists(name: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| path.join(name).exists()))
        .unwrap_or(false)
}

fn has_shared_library(name: &str) -> bool {
    for ldconfig in ["ldconfig", "/usr/sbin/ldconfig", "/sbin/ldconfig"] {
        if let Ok(output) = Command::new(ldconfig).arg("-p").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains(name) {
                    return true;
                }
            }
        }
    }

    shared_library_candidates(name)
        .iter()
        .any(|path| Path::new(path).exists())
}

fn shared_library_candidates(name: &str) -> &'static [&'static str] {
    match name {
        "libcuda.so" => &[
            "/lib/x86_64-linux-gnu/libcuda.so",
            "/usr/lib/x86_64-linux-gnu/libcuda.so",
            "/usr/lib/x86_64-linux-gnu/nvidia/current/libcuda.so",
        ],
        "libcuda.so.1" => &[
            "/lib/x86_64-linux-gnu/libcuda.so.1",
            "/usr/lib/x86_64-linux-gnu/libcuda.so.1",
            "/usr/lib/x86_64-linux-gnu/nvidia/current/libcuda.so.1",
        ],
        "libvulkan.so" => &[
            "/lib/x86_64-linux-gnu/libvulkan.so",
            "/usr/lib/x86_64-linux-gnu/libvulkan.so",
        ],
        "libvulkan.so.1" => &[
            "/lib/x86_64-linux-gnu/libvulkan.so.1",
            "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{recommend_runtime, ExecutionBackend, GpuVendor, RuntimeBundle};

    #[test]
    fn auto_prefers_coreml_on_macos_apple_silicon() {
        let (backend, bundle) = recommend_runtime(
            ExecutionBackend::Auto,
            "macos",
            "aarch64",
            &[],
            false,
            false,
        );

        assert_eq!(backend, ExecutionBackend::CoreMl);
        assert_eq!(bundle, RuntimeBundle::CoreMl);
    }

    #[test]
    fn auto_prefers_cuda_on_linux_nvidia() {
        let (backend, bundle) = recommend_runtime(
            ExecutionBackend::Auto,
            "linux",
            "x86_64",
            &[GpuVendor::Nvidia],
            true,
            true,
        );

        assert_eq!(backend, ExecutionBackend::Cuda);
        assert_eq!(bundle, RuntimeBundle::Cuda12);
    }

    #[test]
    fn explicit_coreml_falls_back_to_cpu_outside_supported_target() {
        let (backend, bundle) = recommend_runtime(
            ExecutionBackend::CoreMl,
            "linux",
            "x86_64",
            &[],
            false,
            false,
        );

        assert_eq!(backend, ExecutionBackend::Cpu);
        assert_eq!(bundle, RuntimeBundle::None);
    }
}
