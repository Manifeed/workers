use std::env;
use std::path::PathBuf;

use directories::BaseDirs;

use crate::error::{Result, WorkerError};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub bin_dir: PathBuf,
}

pub fn app_paths() -> Result<AppPaths> {
    let base_dirs = BaseDirs::new().ok_or_else(|| {
        WorkerError::Config("unable to resolve user base directories".to_string())
    })?;
    let home_dir = base_dirs.home_dir().to_path_buf();
    let (config_dir, data_dir, cache_dir, state_dir, bin_dir) = match env::consts::OS {
        "linux" => (
            env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home_dir.join(".config"))
                .join("manifeed"),
            env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home_dir.join(".local/share"))
                .join("manifeed"),
            env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home_dir.join(".cache"))
                .join("manifeed"),
            env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home_dir.join(".local/state"))
                .join("manifeed"),
            home_dir.join(".local/bin"),
        ),
        "macos" => {
            let config_root = base_dirs.config_dir().join("Manifeed");
            let data_root = base_dirs.data_dir().join("Manifeed");
            (
                config_root,
                data_root.clone(),
                base_dirs.cache_dir().join("Manifeed"),
                data_root.join("state"),
                data_root.join("bin"),
            )
        }
        "windows" => {
            let config_root = base_dirs.config_dir().join("Manifeed");
            let data_root = base_dirs.data_local_dir().join("Manifeed");
            (
                config_root,
                data_root.clone(),
                data_root.join("cache"),
                data_root.join("state"),
                data_root.join("bin"),
            )
        }
        other => {
            return Err(WorkerError::Config(format!(
                "unsupported operating system for worker paths: {other}"
            ))
            .into());
        }
    };
    Ok(AppPaths {
        config_dir,
        data_dir,
        cache_dir,
        state_dir,
        bin_dir,
    })
}

impl AppPaths {
    pub fn version_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("version")
    }
}
