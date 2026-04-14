use std::path::Path;

use manifeed_worker_common::{
    install_user_service, installed_worker_binary_path, ServiceMode, WorkerType, WorkersConfig,
};

use crate::worker_support::service_mode;

pub(super) fn sync_background_service(
    config_path: &Path,
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<(), String> {
    if service_mode(config, worker_type) != ServiceMode::Background {
        return Ok(());
    }

    let installed_binary_path =
        installed_worker_binary_path(worker_type).map_err(|error| error.to_string())?;
    install_user_service(worker_type, &installed_binary_path, config_path)
        .map_err(|error| error.to_string())
}

pub(super) fn reinstall_background_service(
    config_path: &Path,
    worker_type: WorkerType,
) -> Result<(), String> {
    let installed_binary_path =
        installed_worker_binary_path(worker_type).map_err(|error| error.to_string())?;
    install_user_service(worker_type, &installed_binary_path, config_path)
        .map_err(|error| error.to_string())
}
