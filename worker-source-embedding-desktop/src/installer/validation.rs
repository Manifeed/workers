use std::fs;
use std::path::{Path, PathBuf};

use manifeed_worker_common::{WorkerReleaseManifest, WorkerType};
use serde::Deserialize;

use super::runtime::runtime_library_name;

#[derive(Clone, Debug, Deserialize)]
struct BundleManifest {
    product: String,
    version: String,
    worker_version: Option<String>,
    runtime_bundle: Option<String>,
}

pub(super) struct ValidatedBundle {
    pub(super) version: String,
    pub(super) worker_version: Option<String>,
}

pub(super) fn validate_bundle(
    bundle_root: &Path,
    worker_type: WorkerType,
    manifest: &WorkerReleaseManifest,
) -> Result<ValidatedBundle, String> {
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

    Ok(ValidatedBundle {
        version: bundle_manifest.version,
        worker_version: bundle_manifest.worker_version,
    })
}

pub(super) fn find_bundle_root(root: &Path) -> Result<PathBuf, String> {
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

pub(super) fn make_executable(path: &Path) -> Result<(), String> {
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
