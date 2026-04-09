mod bundle;
mod runtime;

use std::fs;
use std::path::{Path, PathBuf};

use manifeed_worker_common::{
    app_paths, install_user_service, installed_worker_binary_path, load_workers_config,
    save_workers_config, uninstall_user_service, ServiceMode, WorkerType, WorkersConfig,
};

use bundle::install_bundle_files;

use crate::worker_support::{mark_worker_installed, mark_worker_removed, service_mode};

pub(crate) use runtime::{manifest_runtime_bundle, resolved_runtime_bundle};

pub(crate) fn install_or_update_worker(
    config_path: &Path,
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<WorkersConfig, String> {
    ensure_api_key_present(config, worker_type)?;

    let installed_bundle = install_bundle_files(config, worker_type)?;
    let runtime_bundle = match worker_type {
        WorkerType::RssScrapper => None,
        WorkerType::SourceEmbedding => Some(resolved_runtime_bundle(config)?),
    };

    let mut stored_config = load_stored_config(config_path)?;
    stored_config.api_url = config.worker_api_url(worker_type).to_string();
    mark_worker_installed(
        &mut stored_config,
        worker_type,
        installed_bundle.version,
        installed_bundle.worker_version,
        runtime_bundle,
    );
    save_workers_config(config_path, &stored_config).map_err(|error| error.to_string())?;

    sync_background_service(config_path, config, worker_type)?;
    Ok(stored_config)
}

pub(crate) fn remove_installed_worker(
    config_path: &Path,
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<WorkersConfig, String> {
    if service_mode(config, worker_type) == ServiceMode::Background {
        uninstall_user_service(worker_type).map_err(|error| error.to_string())?;
    }

    let install_dir = app_paths()
        .map_err(|error| error.to_string())?
        .worker_paths(worker_type)
        .install_dir;
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir).map_err(|error| error.to_string())?;
    }

    let mut stored_config = load_stored_config(config_path)?;
    mark_worker_removed(&mut stored_config, worker_type);
    save_workers_config(config_path, &stored_config).map_err(|error| error.to_string())?;
    Ok(stored_config)
}

pub(crate) fn installed_worker_binary(worker_type: WorkerType) -> Option<PathBuf> {
    installed_worker_binary_path(worker_type)
        .ok()
        .filter(|path| path.exists())
}

fn ensure_api_key_present(config: &WorkersConfig, worker_type: WorkerType) -> Result<(), String> {
    if config.worker_api_key(worker_type).trim().is_empty() {
        return Err(format!(
            "Missing API key for {}. Add it before installing the bundle.",
            worker_type.display_name()
        ));
    }
    Ok(())
}

fn sync_background_service(
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

fn load_stored_config(config_path: &Path) -> Result<WorkersConfig, String> {
    let (_, config) = load_workers_config(Some(config_path)).map_err(|error| error.to_string())?;
    Ok(config)
}
