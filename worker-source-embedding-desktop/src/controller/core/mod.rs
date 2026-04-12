mod bootstrap;
mod process_control;
mod refresh_service;
mod settings_service;
mod snapshot_service;
mod worker_actions;

use std::path::{Path, PathBuf};

use manifeed_worker_common::{WorkerReleaseStatus, WorkerType, WorkersConfig};

use crate::gpu::GpuSupport;
use crate::installer::installed_worker_binary;
use crate::process::external_worker_running;

use super::state::{UiNotice, WorkerRuntimeState};

pub(super) use bootstrap::ConfigAccess;

pub(super) struct AppCore {
    config_path: Option<PathBuf>,
    config: WorkersConfig,
    config_access: ConfigAccess,
    app_release_status: Option<WorkerReleaseStatus>,
    rss: WorkerRuntimeState,
    embedding: WorkerRuntimeState,
    gpu_support: Option<GpuSupport>,
    global_notice: Option<UiNotice>,
    app_busy: bool,
}

impl AppCore {
    pub(super) fn bootstrap() -> Self {
        let (config_path, config, config_access, global_notice) =
            bootstrap::bootstrap_config_state();
        let mut core = Self {
            config_path,
            config,
            config_access,
            app_release_status: None,
            rss: WorkerRuntimeState::default(),
            embedding: WorkerRuntimeState::default(),
            gpu_support: None,
            global_notice,
            app_busy: false,
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

    pub(super) fn is_busy(&self) -> bool {
        self.app_busy
    }

    pub(super) fn is_read_only(&self) -> bool {
        matches!(self.config_access, ConfigAccess::ReadOnly { .. })
    }

    pub(super) fn require_writable(&mut self) -> Result<(), String> {
        match &self.config_access {
            ConfigAccess::Writable => Ok(()),
            ConfigAccess::ReadOnly { reason } => {
                self.global_notice = Some(UiNotice::danger(reason.clone()));
                Err(reason.clone())
            }
        }
    }

    fn config_path(&self) -> Result<&Path, String> {
        self.config_path
            .as_deref()
            .ok_or_else(|| "Workers config path is unavailable.".to_string())
    }

    fn binary_path(&self, worker_type: WorkerType) -> Option<PathBuf> {
        installed_worker_binary(worker_type)
    }

    fn is_installed(&self, worker_type: WorkerType) -> bool {
        self.binary_path(worker_type).is_some()
    }

    pub(in crate::controller) fn is_running(&self, worker_type: WorkerType) -> bool {
        let binary_path = self.binary_path(worker_type);
        self.state(worker_type).child.is_some()
            || external_worker_running(
                worker_type,
                binary_path.as_deref(),
                self.state(worker_type).status_snapshot.as_ref(),
            )
    }

    pub(in crate::controller) fn begin_save(&mut self) {
        self.app_busy = true;
        self.global_notice = Some(UiNotice::neutral("Saving changes..."));
    }

    pub(in crate::controller) fn begin_worker_action(
        &mut self,
        worker_type: WorkerType,
        message: &str,
    ) {
        self.app_busy = true;
        self.state_mut(worker_type).notice = Some(UiNotice::neutral(message));
    }

    pub(in crate::controller) fn begin_update_check(&mut self) {
        self.app_busy = true;
        self.global_notice = Some(UiNotice::neutral_persistent("Checking for updates..."));
    }

    pub(in crate::controller) fn end_action(&mut self) {
        self.app_busy = false;
    }
}

fn stop_child(state: &mut WorkerRuntimeState) {
    if let Some(mut child) = state.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
