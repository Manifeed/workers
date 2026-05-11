use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::github_update::resolve_github_bundle_descriptor;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerReleaseManifest {
    pub artifact_name: String,
    pub family: String,
    pub product: String,
    pub platform: String,
    pub arch: String,
    pub latest_version: String,
    pub minimum_supported_version: String,
    pub worker_version: Option<String>,
    pub artifact_kind: String,
    pub sha256: String,
    pub download_auth: String,
    pub download_url: String,
    pub release_notes_url: String,
    pub published_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseCheckStatus {
    UpToDate,
    UpdateAvailable,
    Incompatible,
    Unverified,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerReleaseStatus {
    pub current_version: String,
    pub platform: String,
    pub arch: String,
    pub status: ReleaseCheckStatus,
    pub manifest: Option<WorkerReleaseManifest>,
    pub checked_at: DateTime<Utc>,
    pub from_cache: bool,
    pub message: Option<String>,
}

pub fn resolve_release_platform() -> String {
    match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => other,
    }
    .to_string()
}

pub fn resolve_release_arch() -> String {
    std::env::consts::ARCH.to_string()
}

pub async fn check_worker_release_status(
    github_repository: &str,
    product: &str,
    current_version: &str,
    cache_path: &Path,
) -> Result<WorkerReleaseStatus> {
    let platform = resolve_release_platform();
    let arch = resolve_release_arch();
    match fetch_manifest_from_github(github_repository, product, &platform, &arch).await {
        Ok(manifest) => {
            persist_manifest_cache(cache_path, &manifest)?;
            Ok(classify_release_status(
                current_version,
                platform,
                arch,
                Some(manifest),
                false,
            ))
        }
        Err(error) => match load_manifest_cache(cache_path) {
            Ok(Some(manifest)) => Ok(classify_release_status(
                current_version,
                platform,
                arch,
                Some(manifest),
                true,
            )),
            _ => Ok(WorkerReleaseStatus {
                current_version: current_version.to_string(),
                platform,
                arch,
                status: ReleaseCheckStatus::Unverified,
                manifest: None,
                checked_at: Utc::now(),
                from_cache: false,
                message: Some(error.user_facing_message()),
            }),
        },
    }
}

async fn fetch_manifest_from_github(
    github_repository: &str,
    product: &str,
    platform: &str,
    arch: &str,
) -> Result<WorkerReleaseManifest> {
    let descriptor = resolve_github_bundle_descriptor(github_repository).await?;
    if !descriptor.artifact_name.contains(product) {
        return Err(crate::error::WorkerError::Version(format!(
            "release artifact {} does not match product {}",
            descriptor.artifact_name, product
        ))
        .into());
    }
    Ok(WorkerReleaseManifest {
        artifact_name: descriptor.artifact_name,
        family: "rss".to_string(),
        product: product.to_string(),
        platform: platform.to_string(),
        arch: arch.to_string(),
        latest_version: descriptor.latest_version.clone(),
        minimum_supported_version: "0.0.0".to_string(),
        worker_version: Some(descriptor.latest_version),
        artifact_kind: "tarball".to_string(),
        sha256: descriptor.sha256,
        download_auth: "public".to_string(),
        download_url: descriptor.download_url,
        release_notes_url: descriptor.html_url,
        published_at: descriptor.published_at,
    })
}

fn load_manifest_cache(cache_path: &Path) -> Result<Option<WorkerReleaseManifest>> {
    if !cache_path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice::<WorkerReleaseManifest>(
        &fs::read(cache_path)?,
    )?))
}

fn persist_manifest_cache(cache_path: &Path, manifest: &WorkerReleaseManifest) -> Result<()> {
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(cache_path, serde_json::to_vec_pretty(manifest)?)?;
    Ok(())
}

fn classify_release_status(
    current_version: &str,
    platform: String,
    arch: String,
    manifest: Option<WorkerReleaseManifest>,
    from_cache: bool,
) -> WorkerReleaseStatus {
    let checked_at = Utc::now();
    let Some(manifest_value) = manifest else {
        return WorkerReleaseStatus {
            current_version: current_version.to_string(),
            platform,
            arch,
            status: ReleaseCheckStatus::Unverified,
            manifest: None,
            checked_at,
            from_cache,
            message: Some("version manifest unavailable".to_string()),
        };
    };
    let status = match (
        Version::parse(current_version),
        Version::parse(&manifest_value.minimum_supported_version),
        Version::parse(&manifest_value.latest_version),
    ) {
        (Ok(current), Ok(minimum), Ok(_)) if current < minimum => ReleaseCheckStatus::Incompatible,
        (Ok(current), _, Ok(latest)) if current < latest => ReleaseCheckStatus::UpdateAvailable,
        (Ok(_), Ok(_), Ok(_)) => ReleaseCheckStatus::UpToDate,
        _ => ReleaseCheckStatus::Unverified,
    };
    let message = match status {
        ReleaseCheckStatus::Incompatible => Some(format!(
            "installed version {} is below minimum supported version {}",
            current_version, manifest_value.minimum_supported_version
        )),
        ReleaseCheckStatus::UpdateAvailable => Some(format!(
            "installed version {} is older than latest version {}",
            current_version, manifest_value.latest_version
        )),
        ReleaseCheckStatus::Unverified => Some("unable to compare semantic versions".to_string()),
        ReleaseCheckStatus::UpToDate => None,
    };
    WorkerReleaseStatus {
        current_version: current_version.to_string(),
        platform,
        arch,
        status,
        manifest: Some(manifest_value),
        checked_at,
        from_cache,
        message,
    }
}
