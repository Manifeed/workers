use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{Result, WorkerError};
use crate::install::install_archive;
use crate::paths::app_paths;
use crate::release::{resolve_release_arch, resolve_release_platform};

#[derive(Clone, Debug)]
pub struct GithubUpdateOptions {
	pub repository: String,
	pub current_version: String,
	pub dry_run: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct GithubUpdateResult {
	pub updated: bool,
	pub current_version: String,
	pub latest_version: String,
	pub asset_name: Option<String>,
	pub installed_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct GithubBundleDescriptor {
	pub artifact_name: String,
	pub download_url: String,
	pub sha256: String,
	pub latest_version: String,
	pub published_at: DateTime<Utc>,
	pub html_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
	tag_name: String,
	assets: Vec<GithubAsset>,
	html_url: String,
	published_at: DateTime<Utc>,
}

#[derive(Clone, Deserialize)]
struct GithubAsset {
	name: String,
	browser_download_url: String,
}

pub async fn resolve_github_bundle_descriptor(repository: &str) -> Result<GithubBundleDescriptor> {
	let release = fetch_latest_release(repository).await?;
	let latest_version = release.tag_name.trim_start_matches('v').to_string();
	let asset = select_release_asset(&release.assets)?;
	let sha_asset = release
		.assets
		.iter()
		.find(|candidate| candidate.name == format!("{}.sha256", asset.name))
		.ok_or_else(|| {
			WorkerError::Version(format!(
				"missing {}.sha256 sibling asset on GitHub release",
				asset.name
			))
		})?;
	let sha256 = fetch_remote_sha256_hex(&sha_asset.browser_download_url).await?;
	Ok(GithubBundleDescriptor {
		artifact_name: asset.name.clone(),
		download_url: asset.browser_download_url.clone(),
		sha256,
		latest_version,
		published_at: release.published_at,
		html_url: release.html_url,
	})
}

pub async fn update_from_github(options: GithubUpdateOptions) -> Result<GithubUpdateResult> {
	let release = fetch_latest_release(&options.repository).await?;
	let latest_version = release.tag_name.trim_start_matches('v').to_string();
	if latest_version == options.current_version {
		return Ok(GithubUpdateResult {
			updated: false,
			current_version: options.current_version,
			latest_version,
			asset_name: None,
			installed_path: None,
		});
	}
	let asset = select_release_asset(&release.assets)?;
	if options.dry_run {
		return Ok(GithubUpdateResult {
			updated: false,
			current_version: options.current_version,
			latest_version,
			asset_name: Some(asset.name),
			installed_path: None,
		});
	}
	let archive_path = download_asset(&asset).await?;
	verify_sha256(&release.assets, &asset, &archive_path).await?;
	let installed_path = install_archive(archive_path.as_path())?;
	Ok(GithubUpdateResult {
		updated: true,
		current_version: options.current_version,
		latest_version,
		asset_name: Some(asset.name),
		installed_path: Some(installed_path),
	})
}

async fn fetch_latest_release(repository: &str) -> Result<GithubRelease> {
	let url = format!("https://api.github.com/repos/{repository}/releases/latest");
	Ok(reqwest::Client::builder()
		.user_agent(format!("crawler_rss/{}", env!("CARGO_PKG_VERSION")))
		.build()?
		.get(url)
		.send()
		.await?
		.error_for_status()?
		.json::<GithubRelease>()
		.await?)
}

fn select_release_asset(assets: &[GithubAsset]) -> Result<GithubAsset> {
	let platform = resolve_release_platform();
	let arch = resolve_release_arch();
	let platform_arch = format!("{platform}-{arch}");
	assets
		.iter()
		.find(|asset| {
			asset.name.starts_with("crawler_rss_bundle-")
				&& asset.name.ends_with(".tar.gz")
				&& asset.name.contains(&platform_arch)
		})
		.cloned()
		.ok_or_else(|| {
			WorkerError::Version(format!(
				"no crawler_rss bundle asset found for {platform_arch}"
			))
			.into()
		})
}

async fn fetch_remote_sha256_hex(download_url: &str) -> Result<String> {
	let expected = reqwest::Client::builder()
		.user_agent(format!("crawler_rss/{}", env!("CARGO_PKG_VERSION")))
		.build()?
		.get(download_url)
		.send()
		.await?
		.error_for_status()?
		.text()
		.await?
		.split_whitespace()
		.next()
		.unwrap_or_default()
		.to_ascii_lowercase();
	if expected.is_empty() {
		return Err(
			WorkerError::Version("empty sha256 checksum from release asset".to_string()).into(),
		);
	}
	Ok(expected)
}

async fn download_asset(asset: &GithubAsset) -> Result<PathBuf> {
	let cache_dir = app_paths()?.cache_dir.join("crawler_rss").join("updates");
	fs::create_dir_all(&cache_dir)?;
	let archive_path = cache_dir.join(&asset.name);
	let bytes = reqwest::Client::builder()
		.user_agent(format!("crawler_rss/{}", env!("CARGO_PKG_VERSION")))
		.build()?
		.get(&asset.browser_download_url)
		.send()
		.await?
		.error_for_status()?
		.bytes()
		.await?;
	fs::write(&archive_path, bytes)?;
	Ok(archive_path)
}

async fn verify_sha256(
	assets: &[GithubAsset],
	asset: &GithubAsset,
	archive_path: &Path,
) -> Result<()> {
	let sha_asset = assets
		.iter()
		.find(|candidate| candidate.name == format!("{}.sha256", asset.name))
		.ok_or_else(|| {
			WorkerError::Version(format!(
				"missing {}.sha256 sibling asset; refusing unverified update",
				asset.name
			))
		})?;
	let expected = fetch_remote_sha256_hex(&sha_asset.browser_download_url).await?;
	let actual = hex::encode(Sha256::digest(fs::read(archive_path)?));
	if actual != expected {
		return Err(WorkerError::Version(format!(
			"downloaded archive sha256 mismatch (expected {expected}, got {actual})"
		))
		.into());
	}
	Ok(())
}
