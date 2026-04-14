use manifeed_worker_common::{ReleaseCheckStatus, WorkerType};

use crate::installer::{install_or_update_worker, remove_installed_worker};
use crate::worker_support::service_mode;

use super::super::state::{UiEdits, UiNotice};
use super::super::utils::summarize_detail;
use super::AppCore;

impl AppCore {
    pub(in crate::controller) fn install_or_update(
        &mut self,
        worker_type: WorkerType,
        edits: UiEdits,
    ) {
        if self.is_running(worker_type) {
            self.state_mut(worker_type).notice = Some(UiNotice::warning(format!(
                "Stop {} before installing an update.",
                worker_type.display_name()
            )));
            return;
        }

        if let Err(error) = self.commit_ui_edits(&edits) {
            self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
            return;
        }

        let config_path = match self.config_path() {
            Ok(path) => path.to_path_buf(),
            Err(error) => {
                self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
                return;
            }
        };

        match install_or_update_worker(&config_path, &self.config, worker_type) {
            Ok(config) => {
                self.config = config;
                self.refresh_release_statuses();
                self.refresh_gpu_support();
                self.refresh_status(worker_type);
                self.state_mut(worker_type).notice = Some(UiNotice::success("Bundle installed."));
            }
            Err(error) => {
                self.state_mut(worker_type).notice = Some(UiNotice::danger(format!(
                    "Could not install bundle. {}",
                    summarize_detail(&error)
                )));
            }
        }
    }

    pub(in crate::controller) fn toggle_run(&mut self, worker_type: WorkerType, edits: UiEdits) {
        if let Err(error) = self.commit_ui_edits(&edits) {
            self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
            return;
        }

        let result = if self.is_running(worker_type) {
            self.stop_worker(worker_type)
        } else {
            self.start_worker(worker_type)
        };

        if let Err(error) = result {
            self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
        }

        self.refresh_status(worker_type);
    }

    pub(in crate::controller) fn uninstall(&mut self, worker_type: WorkerType, edits: UiEdits) {
        let installed_service_mode = service_mode(&self.config, worker_type);

        if let Err(error) = self.commit_ui_edits_for_uninstall(&edits) {
            self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
            return;
        }

        if self.is_running(worker_type) {
            if let Err(error) = self.stop_worker_with_mode(worker_type, installed_service_mode) {
                self.state_mut(worker_type).notice = Some(UiNotice::danger(format!(
                    "Could not stop worker before removing bundle. {}",
                    summarize_detail(&error)
                )));
                return;
            }
            self.state_mut(worker_type).notice = Some(UiNotice::neutral("Removing bundle..."));
        }

        let config_path = match self.config_path() {
            Ok(path) => path.to_path_buf(),
            Err(error) => {
                self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
                return;
            }
        };

        match remove_installed_worker(
            &config_path,
            &self.config,
            worker_type,
            installed_service_mode,
        ) {
            Ok(config) => {
                self.config = config;
                self.refresh_release_statuses();
                self.refresh_gpu_support();
                self.refresh_status(worker_type);
                self.state_mut(worker_type).notice = Some(UiNotice::success("Bundle removed."));
            }
            Err(error) => {
                self.state_mut(worker_type).notice = Some(UiNotice::danger(format!(
                    "Could not remove bundle. {}",
                    summarize_detail(&error)
                )));
            }
        }
    }

    pub(super) fn release_blocked(&self, worker_type: WorkerType) -> bool {
        self.state(worker_type)
            .release_status
            .as_ref()
            .map(|status| {
                matches!(
                    status.status,
                    ReleaseCheckStatus::UpdateAvailable | ReleaseCheckStatus::Incompatible
                )
            })
            .unwrap_or(false)
    }
}
