mod bundle;
mod download;
mod runtime;
mod transaction;
mod validation;

use std::path::{Path, PathBuf};

use manifeed_worker_common::{
    app_paths, install_user_service, installed_worker_binary_path, load_workers_config,
    save_workers_config, uninstall_user_service, ServiceMode, WorkerType, WorkersConfig,
};

use bundle::install_bundle_files;
use transaction::RemovalTransaction;

use crate::worker_support::{mark_worker_installed, mark_worker_removed, service_mode};

pub(crate) use runtime::{manifest_runtime_bundle, resolved_runtime_bundle};

pub(crate) fn install_or_update_worker(
    config_path: &Path,
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<WorkersConfig, String> {
    ensure_api_key_present(config, worker_type)?;

    let runtime_bundle = match worker_type {
        WorkerType::RssScrapper => None,
        WorkerType::SourceEmbedding => Some(resolved_runtime_bundle(config)?),
    };
    let previous_config = load_stored_config(config_path)?;
    let install_transaction = install_bundle_files(config, worker_type)?;

    let mut stored_config = previous_config.clone();
    stored_config.api_url = config.worker_api_url(worker_type).to_string();
    mark_worker_installed(
        &mut stored_config,
        worker_type,
        install_transaction.installed_bundle.version.clone(),
        install_transaction.installed_bundle.worker_version.clone(),
        runtime_bundle,
    );

    if let Err(error) =
        save_workers_config(config_path, &stored_config).map_err(|error| error.to_string())
    {
        return rollback_install_update(config_path, &previous_config, install_transaction, error);
    }

    if let Err(error) = sync_background_service(config_path, &stored_config, worker_type) {
        return rollback_install_update(config_path, &previous_config, install_transaction, error);
    }

    install_transaction.commit();
    Ok(stored_config)
}

pub(crate) fn remove_installed_worker(
    config_path: &Path,
    _config: &WorkersConfig,
    worker_type: WorkerType,
    installed_service_mode: ServiceMode,
) -> Result<WorkersConfig, String> {
    let install_dir = app_paths()
        .map_err(|error| error.to_string())?
        .worker_paths(worker_type)
        .install_dir;
    let removal = RemovalTransaction::stage(&install_dir)?;

    let previous_config = load_stored_config(config_path)?;
    let mut stored_config = previous_config.clone();
    mark_worker_removed(&mut stored_config, worker_type);

    if let Err(error) =
        save_workers_config(config_path, &stored_config).map_err(|error| error.to_string())
    {
        return rollback_uninstall(
            config_path,
            &previous_config,
            worker_type,
            installed_service_mode,
            &removal,
            false,
            error,
        );
    }

    let mut service_removed = false;
    if installed_service_mode == ServiceMode::Background {
        if let Err(error) = uninstall_user_service(worker_type).map_err(|error| error.to_string()) {
            return rollback_uninstall(
                config_path,
                &previous_config,
                worker_type,
                installed_service_mode,
                &removal,
                service_removed,
                error,
            );
        }
        service_removed = true;
    }

    if let Err(error) = removal.commit() {
        return rollback_uninstall(
            config_path,
            &previous_config,
            worker_type,
            installed_service_mode,
            &removal,
            service_removed,
            error,
        );
    }

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

fn rollback_install_update(
    config_path: &Path,
    previous_config: &WorkersConfig,
    install_transaction: transaction::InstalledBundleTransaction,
    primary_error: String,
) -> Result<WorkersConfig, String> {
    let rollback_install_error = install_transaction.rollback().err();
    let rollback_config_error = save_workers_config(config_path, previous_config)
        .map_err(|error| error.to_string())
        .err();

    let mut combined_error = primary_error;
    if let Some(error) = rollback_install_error {
        combined_error.push_str(&format!(" Bundle rollback failed: {error}."));
    }
    if let Some(error) = rollback_config_error {
        combined_error.push_str(&format!(" Config rollback failed: {error}."));
    }

    Err(combined_error)
}

fn rollback_uninstall(
    config_path: &Path,
    previous_config: &WorkersConfig,
    worker_type: WorkerType,
    installed_service_mode: ServiceMode,
    removal: &RemovalTransaction,
    service_removed: bool,
    primary_error: String,
) -> Result<WorkersConfig, String> {
    let rollback_install_error = removal.rollback().err();
    let rollback_config_error = save_workers_config(config_path, previous_config)
        .map_err(|error| error.to_string())
        .err();
    let rollback_service_error =
        if service_removed && installed_service_mode == ServiceMode::Background {
            reinstall_background_service(config_path, worker_type).err()
        } else {
            None
        };

    let mut combined_error = primary_error;
    if let Some(error) = rollback_install_error {
        combined_error.push_str(&format!(" Bundle rollback failed: {error}."));
    }
    if let Some(error) = rollback_config_error {
        combined_error.push_str(&format!(" Config rollback failed: {error}."));
    }
    if let Some(error) = rollback_service_error {
        combined_error.push_str(&format!(" Service rollback failed: {error}."));
    }

    Err(combined_error)
}

fn reinstall_background_service(config_path: &Path, worker_type: WorkerType) -> Result<(), String> {
    let installed_binary_path =
        installed_worker_binary_path(worker_type).map_err(|error| error.to_string())?;
    install_user_service(worker_type, &installed_binary_path, config_path)
        .map_err(|error| error.to_string())
}
