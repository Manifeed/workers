mod bundle;
mod config_state;
mod download;
mod rollback;
mod runtime;
mod service_sync;
mod transaction;
mod validation;

use std::path::{Path, PathBuf};

use manifeed_worker_common::{
    app_paths, save_workers_config, uninstall_user_service, ServiceMode, WorkerType, WorkersConfig,
};

use bundle::install_bundle_files;
use config_state::{ensure_api_key_present, load_stored_config};
use rollback::{rollback_install_update, rollback_uninstall};
use service_sync::sync_background_service;
use transaction::RemovalTransaction;

use crate::worker_support::{mark_worker_installed, mark_worker_removed};

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
    config_state::installed_worker_binary(worker_type)
}
