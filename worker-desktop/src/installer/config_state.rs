use std::path::{Path, PathBuf};

use manifeed_worker_common::{
    installed_worker_binary_path, load_workers_config, WorkerType, WorkersConfig,
};

pub(super) fn installed_worker_binary(worker_type: WorkerType) -> Option<PathBuf> {
    installed_worker_binary_path(worker_type)
        .ok()
        .filter(|path| path.exists())
}

pub(super) fn ensure_api_key_present(
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<(), String> {
    if config.worker_api_key(worker_type).trim().is_empty() {
        return Err(format!(
            "Missing API key for {}. Add it before installing the bundle.",
            worker_type.display_name()
        ));
    }
    Ok(())
}

pub(super) fn load_stored_config(config_path: &Path) -> Result<WorkersConfig, String> {
    let (_, config) = load_workers_config(Some(config_path)).map_err(|error| error.to_string())?;
    Ok(config)
}
