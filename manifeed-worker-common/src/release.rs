use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerReleaseManifest {
    pub product: String,
    pub platform: String,
    pub arch: String,
    pub latest_version: String,
    pub minimum_supported_version: String,
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

pub fn check_worker_release_status(
    api_url: &str,
    product: &str,
    current_version: &str,
    cache_path: &Path,
) -> Result<WorkerReleaseStatus> {
    let platform = resolve_release_platform();
    let arch = resolve_release_arch();

    match fetch_manifest(api_url, product, &platform, &arch) {
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
            Ok(None) => Ok(WorkerReleaseStatus {
                current_version: current_version.to_string(),
                platform,
                arch,
                status: ReleaseCheckStatus::Unverified,
                manifest: None,
                checked_at: Utc::now(),
                from_cache: false,
                message: Some(format!("version check unavailable: {error}")),
            }),
            Err(cache_error) => Ok(WorkerReleaseStatus {
                current_version: current_version.to_string(),
                platform,
                arch,
                status: ReleaseCheckStatus::Unverified,
                manifest: None,
                checked_at: Utc::now(),
                from_cache: false,
                message: Some(format!(
                    "version check unavailable: {error}; cache read failed: {cache_error}"
                )),
            }),
        },
    }
}

fn fetch_manifest(
    api_url: &str,
    product: &str,
    platform: &str,
    arch: &str,
) -> Result<WorkerReleaseManifest> {
    let response = Client::new()
        .get(format!(
            "{}/workers/releases/manifest",
            api_url.trim_end_matches('/')
        ))
        .query(&[("product", product), ("platform", platform), ("arch", arch)])
        .send()?;

    let status = response.status();
    if status != StatusCode::OK {
        let body = response.text().unwrap_or_else(|_| String::new());
        return Err(crate::error::WorkerError::Api {
            status: status.as_u16(),
            message: body,
        });
    }

    Ok(response.json::<WorkerReleaseManifest>()?)
}

fn load_manifest_cache(cache_path: &Path) -> Result<Option<WorkerReleaseManifest>> {
    if !cache_path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(cache_path)?;
    Ok(Some(serde_json::from_slice::<WorkerReleaseManifest>(
        &bytes,
    )?))
}

fn persist_manifest_cache(cache_path: &Path, manifest: &WorkerReleaseManifest) -> Result<()> {
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(manifest)?;
    fs::write(cache_path, payload)?;
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
        (Ok(current), Ok(minimum), Ok(_latest)) if current < minimum => {
            ReleaseCheckStatus::Incompatible
        }
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
