use std::env;
use std::path::PathBuf;

use directories::BaseDirs;

use crate::error::{Result, WorkerError};
use crate::types::WorkerType;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub bin_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct WorkerRuntimePaths {
    pub install_dir: PathBuf,
    pub status_file: PathBuf,
    pub log_file: PathBuf,
    pub cache_dir: PathBuf,
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
            let cache_root = base_dirs.cache_dir().join("Manifeed");
            let bin_root = data_root.join("bin");
            (
                config_root.clone(),
                data_root.clone(),
                cache_root,
                data_root.join("state"),
                bin_root,
            )
        }
        "windows" => {
            let config_root = base_dirs.config_dir().join("Manifeed");
            let data_root = base_dirs.data_local_dir().join("Manifeed");
            let cache_root = data_root.join("cache");
            let bin_root = data_root.join("bin");
            (
                config_root,
                data_root.clone(),
                cache_root,
                data_root.join("state"),
                bin_root,
            )
        }
        other => {
            return Err(WorkerError::Config(format!(
                "unsupported operating system for worker paths: {other}"
            )));
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
    pub fn workers_config_file(&self) -> PathBuf {
        self.config_dir.join("workers.json")
    }

    pub fn version_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("version")
    }

    pub fn desktop_install_dir(&self) -> PathBuf {
        self.data_dir.join("desktop")
    }

    pub fn worker_paths(&self, worker_type: WorkerType) -> WorkerRuntimePaths {
        let install_dir = self
            .data_dir
            .join(worker_type.section_name())
            .join("current");
        let log_dir = self.cache_dir.join(worker_type.section_name());
        let state_dir = self.state_dir.join(worker_type.section_name());
        let cache_dir = match worker_type {
            WorkerType::RssScrapper => self.cache_dir.join("rss"),
            WorkerType::SourceEmbedding => self.cache_dir.join("worker-source-embedding/models"),
        };

        WorkerRuntimePaths {
            install_dir,
            status_file: state_dir.join("status.json"),
            log_file: log_dir.join("worker.log"),
            cache_dir,
        }
    }
}

pub fn installed_worker_binary_path(worker_type: WorkerType) -> Result<PathBuf> {
    Ok(app_paths()?
        .worker_paths(worker_type)
        .install_dir
        .join("bin")
        .join(worker_type.binary_name()))
}

pub fn installed_embedding_runtime_dir() -> Result<PathBuf> {
    Ok(app_paths()?
        .worker_paths(WorkerType::SourceEmbedding)
        .install_dir
        .join("runtime"))
}

pub fn installed_embedding_runtime_library_path() -> Result<PathBuf> {
    Ok(installed_embedding_runtime_dir()?
        .join("lib")
        .join(if env::consts::OS == "macos" {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        }))
}

pub fn installed_embedding_runtime_bundle_marker_path() -> Result<PathBuf> {
    Ok(installed_embedding_runtime_dir()?.join("bundle.txt"))
}
