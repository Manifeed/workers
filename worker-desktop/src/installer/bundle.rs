use std::fs;
use std::path::Path;
use std::process::Command;

use chrono::Utc;
use manifeed_worker_common::{
    app_paths, check_worker_release_status_with_runtime, WorkerReleaseManifest, WorkerType,
    WorkersConfig,
};

use super::download::download_to_path;
use super::runtime::manifest_runtime_bundle;
use super::transaction::replace_current_installation;
use super::validation::{find_bundle_root, make_executable, validate_bundle};
use crate::worker_support::{installed_version, release_cache_name};

#[derive(Clone, Debug)]
pub(super) struct InstalledBundle {
    pub(super) version: String,
    pub(super) worker_version: Option<String>,
}

pub(super) use super::transaction::InstalledBundleTransaction;

pub(super) fn install_bundle_files(
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<InstalledBundleTransaction, String> {
    let api_url = config.worker_api_url(worker_type).to_string();
    let manifest = fetch_worker_manifest(&api_url, config, worker_type)?;
    let bundle_runtime = manifest.runtime_bundle.clone();
    let app_dirs = app_paths().map_err(|error| error.to_string())?;
    let temp_root = app_dirs.cache_dir.join("installer");
    fs::create_dir_all(&temp_root).map_err(|error| error.to_string())?;

    let download_name = format!(
        "{}-{}.tar.gz",
        manifest.product,
        bundle_runtime.as_deref().unwrap_or("default")
    );
    let download_path = temp_root.join(download_name);
    download_to_path(
        &manifest.download_url,
        destination_api_key(config, worker_type),
        &download_path,
        &manifest.sha256,
    )?;

    let worker_paths = app_dirs.worker_paths(worker_type);
    let install_parent = worker_paths
        .install_dir
        .parent()
        .ok_or_else(|| "Parent installation directory not found".to_string())?
        .to_path_buf();
    fs::create_dir_all(&install_parent).map_err(|error| error.to_string())?;

    let staging_dir = install_parent.join(format!(".staging-{}", Utc::now().timestamp_millis()));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging_dir).map_err(|error| error.to_string())?;

    let result = (|| {
        extract_archive(&download_path, &staging_dir)?;
        let bundle_root = find_bundle_root(&staging_dir)?;
        let validated_bundle = validate_bundle(&bundle_root, worker_type, &manifest)?;
        let binary_path = bundle_root.join("bin").join(worker_type.binary_name());
        make_executable(&binary_path)?;
        let (current_dir, backup_dir) =
            replace_current_installation(&install_parent, &bundle_root)?;

        Ok(InstalledBundleTransaction::new(
            InstalledBundle {
                version: validated_bundle.version,
                worker_version: validated_bundle.worker_version,
            },
            current_dir,
            backup_dir,
        ))
    })();

    if staging_dir.exists() {
        let _ = fs::remove_dir_all(&staging_dir);
    }

    result
}

fn fetch_worker_manifest(
    api_url: &str,
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<WorkerReleaseManifest, String> {
    let runtime_bundle = manifest_runtime_bundle(config, worker_type)?;
    let cache_path = app_paths()
        .map_err(|error| error.to_string())?
        .version_cache_dir()
        .join(release_cache_name(worker_type, runtime_bundle.as_deref()));

    let release = check_worker_release_status_with_runtime(
        api_url,
        worker_type.cli_product(),
        installed_version(config, worker_type),
        runtime_bundle.as_deref(),
        &cache_path,
    )
    .map_err(|error| error.to_string())?;

    release.manifest.ok_or_else(|| {
        release.message.clone().unwrap_or_else(|| {
            "Update check is unavailable. Verify the API URL and try again.".to_string()
        })
    })
}

fn destination_api_key(config: &WorkersConfig, worker_type: WorkerType) -> Option<&str> {
    let api_key = config.worker_api_key(worker_type).trim();
    if api_key.is_empty() {
        None
    } else {
        Some(api_key)
    }
}

fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(destination)
        .status()
        .map_err(|error| error.to_string())?;

    if status.success() {
        return Ok(());
    }

    Err(format!(
        "Could not extract {} ({status})",
        archive_path.display()
    ))
}
