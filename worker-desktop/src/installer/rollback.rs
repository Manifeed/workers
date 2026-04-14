use std::path::Path;

use manifeed_worker_common::{save_workers_config, ServiceMode, WorkerType, WorkersConfig};

use super::service_sync::reinstall_background_service;
use super::transaction::{InstalledBundleTransaction, RemovalTransaction};

pub(super) fn rollback_install_update(
    config_path: &Path,
    previous_config: &WorkersConfig,
    install_transaction: InstalledBundleTransaction,
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

pub(super) fn rollback_uninstall(
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
