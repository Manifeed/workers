use manifeed_worker_common::{app_paths, AccelerationMode, WorkerType};
use worker_source_embedding::runtime::{onnxruntime_dylib_name, probe_system, ExecutionBackend};

use crate::cli::ProbeArgs;

use super::shared::{map_provider_arg, resolve_probe_acceleration_mode};

pub(crate) fn probe_command(args: ProbeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let acceleration_mode =
        resolve_probe_acceleration_mode(args.config.as_deref(), args.acceleration)?;
    let provider = args
        .provider
        .map(map_provider_arg)
        .unwrap_or_else(|| match acceleration_mode {
            AccelerationMode::Auto => ExecutionBackend::Auto,
            AccelerationMode::Cpu => ExecutionBackend::Cpu,
            AccelerationMode::Gpu => ExecutionBackend::Auto,
        });
    let ort_dylib_path = app_paths()?
        .worker_paths(WorkerType::SourceEmbedding)
        .install_dir
        .join("runtime/lib")
        .join(onnxruntime_dylib_name());
    let probe = probe_system(
        provider,
        ort_dylib_path.exists().then_some(ort_dylib_path.as_path()),
    );
    println!("{}", serde_json::to_string_pretty(&probe)?);
    Ok(())
}
