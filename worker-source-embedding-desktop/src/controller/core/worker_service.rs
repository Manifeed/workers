use std::fs::{self, OpenOptions};
use std::process::{Command as ProcessCommand, Stdio};

use manifeed_worker_common::{
    app_paths, start_user_service, stop_user_service, AccelerationMode, ReleaseCheckStatus,
    ServiceMode, WorkerRuntimePaths, WorkerType,
};

use crate::installer::{install_or_update_worker, remove_installed_worker};
use crate::process::{external_worker_running, terminate_process};
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
        self.begin_worker_action(worker_type, "Preparing installation...");

        if let Err(error) = self.apply_edits(&edits, true) {
            self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
            self.end_action();
            return;
        }

        match install_or_update_worker(&self.config_path, &self.config, worker_type) {
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

        self.end_action();
    }

    pub(in crate::controller) fn toggle_run(&mut self, worker_type: WorkerType, edits: UiEdits) {
        let verb = if self.is_running(worker_type) {
            "Stopping worker..."
        } else {
            "Starting worker..."
        };
        self.begin_worker_action(worker_type, verb);

        if let Err(error) = self.apply_edits(&edits, true) {
            self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
            self.end_action();
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
        self.end_action();
    }

    pub(in crate::controller) fn uninstall(&mut self, worker_type: WorkerType, edits: UiEdits) {
        self.begin_worker_action(worker_type, "Removing bundle...");

        if let Err(error) = self.apply_edits(&edits, true) {
            self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
            self.end_action();
            return;
        }

        match remove_installed_worker(&self.config_path, &self.config, worker_type) {
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

        self.end_action();
    }

    fn release_blocked(&self, worker_type: WorkerType) -> bool {
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

    fn start_worker(&mut self, worker_type: WorkerType) -> Result<(), String> {
        if self.release_blocked(worker_type) {
            return Err(format!(
                "{} needs an update before it can start.",
                worker_type.display_name()
            ));
        }

        if worker_type == WorkerType::SourceEmbedding
            && self.config.embedding.acceleration_mode == AccelerationMode::Gpu
        {
            let gpu = self.gpu_support.clone().unwrap_or_default();
            if !gpu.is_supported() {
                return Err(format!(
                    "Could not start worker. {}",
                    summarize_detail(&gpu.summary())
                ));
            }
        }

        if service_mode(&self.config, worker_type) == ServiceMode::Background {
            start_user_service(worker_type).map_err(|error| {
                format!(
                    "Could not start worker. {}",
                    summarize_detail(&error.to_string())
                )
            })?;
            self.state_mut(worker_type).notice = Some(UiNotice::success("Worker started."));
            return Ok(());
        }

        if external_worker_running(self.state(worker_type).status_snapshot.as_ref())
            && self.state(worker_type).child.is_none()
        {
            return Err(format!(
                "Could not start worker. Another {} process is already running.",
                worker_type.display_name()
            ));
        }

        let Some(binary) = self.binary_path(worker_type) else {
            return Err(format!(
                "Could not start worker. {} is not installed.",
                worker_type.display_name()
            ));
        };

        let Some(paths) = self.runtime_paths(worker_type) else {
            return Err("Could not start worker. Local runtime paths are unavailable.".to_string());
        };

        if let Some(parent) = paths.log_file.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not start worker. {}",
                    summarize_detail(&error.to_string())
                )
            })?;
        }

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.log_file)
            .map_err(|error| {
                format!(
                    "Could not start worker. {}",
                    summarize_detail(&error.to_string())
                )
            })?;

        let stderr = stdout.try_clone().map_err(|error| {
            format!(
                "Could not start worker. {}",
                summarize_detail(&error.to_string())
            )
        })?;

        let mut command = ProcessCommand::new(&binary);
        command
            .arg("run")
            .arg("--config")
            .arg(&self.config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));

        let child = command.spawn().map_err(|error| {
            format!(
                "Could not start worker. {}",
                summarize_detail(&error.to_string())
            )
        })?;

        self.state_mut(worker_type).child = Some(child);
        self.state_mut(worker_type).notice = Some(UiNotice::success("Worker started."));
        Ok(())
    }

    pub(super) fn stop_worker(&mut self, worker_type: WorkerType) -> Result<(), String> {
        if service_mode(&self.config, worker_type) == ServiceMode::Background {
            stop_user_service(worker_type).map_err(|error| {
                format!(
                    "Could not stop worker. {}",
                    summarize_detail(&error.to_string())
                )
            })?;
            self.state_mut(worker_type).notice = Some(UiNotice::success("Worker stopped."));
            return Ok(());
        }

        if let Some(mut child) = self.state_mut(worker_type).child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.state_mut(worker_type).notice = Some(UiNotice::success("Worker stopped."));
            return Ok(());
        }

        if let Some(snapshot) = self.state(worker_type).status_snapshot.as_ref() {
            if external_worker_running(Some(snapshot)) {
                terminate_process(snapshot.pid).map_err(|error| {
                    format!("Could not stop worker. {}", summarize_detail(&error))
                })?;
                self.state_mut(worker_type).notice = Some(UiNotice::success("Worker stopped."));
                return Ok(());
            }
        }

        self.state_mut(worker_type).notice = Some(UiNotice::neutral("Worker is already stopped."));
        Ok(())
    }

    fn runtime_paths(&self, worker_type: WorkerType) -> Option<WorkerRuntimePaths> {
        app_paths()
            .ok()
            .map(|paths| paths.worker_paths(worker_type))
    }
}
