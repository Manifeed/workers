use std::fs::{self, OpenOptions};
use std::process::{Command as ProcessCommand, Stdio};

use manifeed_worker_common::{
    app_paths, start_user_service, stop_user_service, AccelerationMode, ServiceMode,
    WorkerRuntimePaths, WorkerType,
};

use crate::process::{external_worker_running, terminate_process};
use crate::worker_support::service_mode;

use super::super::state::UiNotice;
use super::super::utils::summarize_detail;
use super::AppCore;

impl AppCore {
    pub(super) fn start_worker(&mut self, worker_type: WorkerType) -> Result<(), String> {
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

        let binary_path = self.binary_path(worker_type);
        if external_worker_running(
            worker_type,
            binary_path.as_deref(),
            self.state(worker_type).status_snapshot.as_ref(),
        ) && self.state(worker_type).child.is_none()
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
            .arg(self.config_path()?)
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
        let current_mode = service_mode(&self.config, worker_type);
        self.stop_worker_with_mode(worker_type, current_mode)
    }

    pub(super) fn stop_worker_with_mode(
        &mut self,
        worker_type: WorkerType,
        current_mode: ServiceMode,
    ) -> Result<(), String> {
        if current_mode == ServiceMode::Background {
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
            let binary_path = self.binary_path(worker_type);
            if external_worker_running(worker_type, binary_path.as_deref(), Some(snapshot)) {
                terminate_process(worker_type, snapshot.pid, binary_path.as_deref()).map_err(
                    |error| format!("Could not stop worker. {}", summarize_detail(&error)),
                )?;
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
