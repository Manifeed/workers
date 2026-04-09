use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use manifeed_worker_common::{
    app_paths, check_worker_release_status_with_runtime, WorkerReleaseManifest, WorkerType,
    WorkersConfig,
};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::runtime::{manifest_runtime_bundle, runtime_library_name};
use crate::worker_support::{installed_version, release_cache_name};

#[derive(Clone, Debug, Deserialize)]
struct BundleManifest {
    product: String,
    version: String,
    worker_version: Option<String>,
    runtime_bundle: Option<String>,
}

pub(super) struct InstalledBundle {
    pub(super) version: String,
    pub(super) worker_version: Option<String>,
}

pub(super) fn install_bundle_files(
    config: &WorkersConfig,
    worker_type: WorkerType,
) -> Result<InstalledBundle, String> {
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
    )?;
    verify_sha256_if_present(&download_path, &manifest.sha256)?;

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
        let bundle_manifest = validate_bundle(&bundle_root, worker_type, &manifest)?;
        let binary_path = bundle_root.join("bin").join(worker_type.binary_name());
        make_executable(&binary_path)?;
        replace_current_installation(&install_parent, &bundle_root)?;

        Ok(InstalledBundle {
            version: bundle_manifest.version,
            worker_version: bundle_manifest.worker_version,
        })
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

fn validate_bundle(
    bundle_root: &Path,
    worker_type: WorkerType,
    manifest: &WorkerReleaseManifest,
) -> Result<BundleManifest, String> {
    let manifest_path = bundle_root.join("manifest.json");
    let payload = fs::read(&manifest_path).map_err(|error| error.to_string())?;
    let bundle_manifest =
        serde_json::from_slice::<BundleManifest>(&payload).map_err(|error| error.to_string())?;

    if bundle_manifest.product != worker_type.cli_product() {
        return Err(format!(
            "Invalid bundle {} for {}",
            bundle_manifest.product,
            worker_type.display_name()
        ));
    }
    if bundle_manifest.version.trim().is_empty() {
        return Err("Bundle version is missing".to_string());
    }
    if bundle_manifest.version != manifest.latest_version {
        return Err(format!(
            "Unexpected bundle version {}. Manifest version is {}",
            bundle_manifest.version, manifest.latest_version
        ));
    }
    let expected_worker_version = manifest
        .worker_version
        .as_deref()
        .unwrap_or(manifest.latest_version.as_str());
    let actual_worker_version = bundle_manifest
        .worker_version
        .as_deref()
        .unwrap_or(bundle_manifest.version.as_str());
    if actual_worker_version != expected_worker_version {
        return Err(format!(
            "Unexpected bundle worker version {}. Manifest worker version is {}",
            actual_worker_version, expected_worker_version
        ));
    }

    let binary_path = bundle_root.join("bin").join(worker_type.binary_name());
    if !binary_path.exists() {
        return Err(format!(
            "Bundle is missing the {} binary",
            worker_type.binary_name()
        ));
    }

    if worker_type == WorkerType::SourceEmbedding && manifest.runtime_bundle.is_some() {
        let runtime_library = bundle_root.join("runtime/lib").join(runtime_library_name());
        if !runtime_library.exists() {
            return Err("Embedding bundle is missing the ONNX runtime".to_string());
        }
    }

    if bundle_manifest.runtime_bundle != manifest.runtime_bundle {
        return Err("Bundle runtime does not match the manifest runtime".to_string());
    }

    Ok(bundle_manifest)
}

fn replace_current_installation(install_parent: &Path, bundle_root: &Path) -> Result<(), String> {
    let current_dir = install_parent.join("current");
    let backup_dir = install_parent.join(".backup-current");

    if backup_dir.exists() {
        fs::remove_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    }
    if current_dir.exists() {
        fs::rename(&current_dir, &backup_dir).map_err(|error| error.to_string())?;
    }

    if let Err(error) = fs::rename(bundle_root, &current_dir) {
        if backup_dir.exists() {
            let _ = fs::rename(&backup_dir, &current_dir);
        }
        return Err(error.to_string());
    }

    if backup_dir.exists() {
        fs::remove_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn find_bundle_root(root: &Path) -> Result<PathBuf, String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let manifest_path = current.join("manifest.json");
        if manifest_path.exists() {
            return Ok(current);
        }

        let entries = fs::read_dir(&current).map_err(|error| error.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            }
        }
    }

    Err(format!(
        "manifest.json was not found in archive {}",
        root.display()
    ))
}

fn destination_api_key<'a>(config: &'a WorkersConfig, worker_type: WorkerType) -> Option<&'a str> {
    let api_key = config.worker_api_key(worker_type).trim();
    if api_key.is_empty() {
        None
    } else {
        Some(api_key)
    }
}

fn download_to_path(
    download_url: &str,
    bearer_token: Option<&str>,
    destination: &Path,
) -> Result<(), String> {
    let mut request = Client::new().get(download_url);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    let response = request.send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Download failed for {} ({})",
            download_url,
            response.status()
        ));
    }

    let bytes = response.bytes().map_err(|error| error.to_string())?;
    let mut file = fs::File::create(destination).map_err(|error| error.to_string())?;
    file.write_all(bytes.as_ref())
        .map_err(|error| error.to_string())
}

fn verify_sha256_if_present(path: &Path, expected_sha256: &str) -> Result<(), String> {
    let normalized = expected_sha256.trim().to_ascii_lowercase();
    if normalized.len() != 64
        || !normalized
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Ok(());
    }

    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let digest = Sha256::digest(bytes);
    let actual = format!("{:x}", digest);
    if actual == normalized {
        return Ok(());
    }

    Err(format!(
        "Invalid sha256 for {}: expected {}, got {}",
        path.display(),
        normalized,
        actual
    ))
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

fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
    }
    Ok(())
}
