mod refresh_service;
mod settings_service;
mod snapshot_service;
mod worker_service;

use std::path::PathBuf;

use manifeed_worker_common::{load_workers_config, save_workers_config, WorkerType, WorkersConfig};

use crate::gpu::GpuSupport;
use crate::installer::installed_worker_binary;
use crate::process::external_worker_running;

use super::state::APP_VERSION;
use super::state::{UiNotice, WorkerRuntimeState};

pub(super) struct AppCore {
    config_path: PathBuf,
    config: WorkersConfig,
    app_release_status: Option<manifeed_worker_common::WorkerReleaseStatus>,
    rss: WorkerRuntimeState,
    embedding: WorkerRuntimeState,
    gpu_support: Option<GpuSupport>,
    global_notice: Option<UiNotice>,
    app_busy: bool,
    busy_worker: Option<WorkerType>,
}

impl AppCore {
    pub(super) fn bootstrap() -> Self {
        let (config_path, config) = load_workers_config(None).unwrap_or_else(|_| {
            (
                PathBuf::from("workers.json"),
                manifeed_worker_common::WorkersConfig::default(),
            )
        });
        let mut config = config;
        if config.desktop_installed_version != APP_VERSION {
            config.desktop_installed_version = APP_VERSION.to_string();
            let _ = save_workers_config(&config_path, &config);
        }

        let mut core = Self {
            config_path,
            config,
            app_release_status: None,
            rss: WorkerRuntimeState::default(),
            embedding: WorkerRuntimeState::default(),
            gpu_support: None,
            global_notice: None,
            app_busy: false,
            busy_worker: None,
        };
        core.refresh();
        core
    }

    pub(super) fn stop_all_children(&mut self) {
        stop_child(&mut self.rss);
        stop_child(&mut self.embedding);
    }

    fn state(&self, worker_type: WorkerType) -> &WorkerRuntimeState {
        match worker_type {
            WorkerType::RssScrapper => &self.rss,
            WorkerType::SourceEmbedding => &self.embedding,
        }
    }

    fn state_mut(&mut self, worker_type: WorkerType) -> &mut WorkerRuntimeState {
        match worker_type {
            WorkerType::RssScrapper => &mut self.rss,
            WorkerType::SourceEmbedding => &mut self.embedding,
        }
    }

    fn binary_path(&self, worker_type: WorkerType) -> Option<PathBuf> {
        installed_worker_binary(worker_type)
    }

    fn is_installed(&self, worker_type: WorkerType) -> bool {
        self.binary_path(worker_type).is_some()
    }

    fn is_running(&self, worker_type: WorkerType) -> bool {
        self.state(worker_type).child.is_some()
            || external_worker_running(self.state(worker_type).status_snapshot.as_ref())
    }

    fn begin_save(&mut self) {
        self.app_busy = true;
        self.busy_worker = None;
        self.global_notice = Some(UiNotice::neutral("Saving changes..."));
    }

    fn begin_worker_action(&mut self, worker_type: WorkerType, message: &str) {
        self.app_busy = true;
        self.busy_worker = Some(worker_type);
        self.state_mut(worker_type).notice = Some(UiNotice::neutral(message));
    }

    fn end_action(&mut self) {
        self.app_busy = false;
        self.busy_worker = None;
    }
}

fn stop_child(state: &mut WorkerRuntimeState) {
    if let Some(mut child) = state.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
