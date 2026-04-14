use std::env;
use std::path::Path;

use super::{
    command_exists, detect_gpu_vendors, detect_render_node, format_execution_backends,
    has_shared_library, ort_dylib_candidates, probe_runtime_support, read_os_release,
    ExecutionBackend, GpuVendor, RuntimeBundle, RuntimeProbe,
};

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

    let (recommended_backend, recommended_runtime_bundle) =
        recommend_runtime(preferred_backend, &os, &arch, &gpu_vendors, has_cuda_driver);
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

pub(crate) fn recommend_runtime(
    preferred_backend: ExecutionBackend,
    os: &str,
    arch: &str,
    gpu_vendors: &[GpuVendor],
    has_cuda_driver: bool,
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

#[cfg(test)]
mod tests {
    use super::{recommend_runtime, ExecutionBackend, GpuVendor, RuntimeBundle};

    #[test]
    fn auto_prefers_coreml_on_macos_apple_silicon() {
        let (backend, bundle) =
            recommend_runtime(ExecutionBackend::Auto, "macos", "aarch64", &[], false);

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
        );

        assert_eq!(backend, ExecutionBackend::Cuda);
        assert_eq!(bundle, RuntimeBundle::Cuda12);
    }

    #[test]
    fn explicit_coreml_falls_back_to_cpu_outside_supported_target() {
        let (backend, bundle) =
            recommend_runtime(ExecutionBackend::CoreMl, "linux", "x86_64", &[], false);

        assert_eq!(backend, ExecutionBackend::Cpu);
        assert_eq!(bundle, RuntimeBundle::None);
    }
}
