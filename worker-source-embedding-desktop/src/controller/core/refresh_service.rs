use std::fs;
use std::io;
use std::time::Instant;

use manifeed_worker_common::{
    app_paths, check_worker_release_status, check_worker_release_status_with_runtime, WorkerPhase,
    WorkerReleaseStatus, WorkerStatusSnapshot, WorkerType,
};

use crate::gpu::GpuSupport;
use crate::installer::manifest_runtime_bundle;
use crate::worker_support::{
    installed_version, release_cache_name, release_cache_name_for_product, ALL_WORKERS,
};

use super::super::state::{UiNotice, APP_VERSION};
use super::super::utils::{
    predicted_gpu_support, summarize_detail, worker_is_busy, worker_requires_update,
};
use super::AppCore;

impl AppCore {
    pub(in crate::controller) fn refresh(&mut self) {
        for worker_type in ALL_WORKERS {
            self.poll_child(worker_type);
            self.refresh_status(worker_type);
        }
        self.clear_expired_notices();
        for worker_type in ALL_WORKERS {
            self.reconcile_outdated_worker(worker_type);
        }
    }

    pub(in crate::controller) fn refresh_release_statuses(&mut self) {
        self.app_release_status = self.compute_app_release_status();
        for worker_type in ALL_WORKERS {
            let status = self.compute_release_status(worker_type);
            self.state_mut(worker_type).release_status = status;
        }
        for worker_type in ALL_WORKERS {
            self.reconcile_outdated_worker(worker_type);
        }
    }

    pub(in crate::controller) fn refresh_gpu_support(&mut self) {
        self.gpu_support = self
            .binary_path(WorkerType::SourceEmbedding)
            .map(|binary| GpuSupport::probe(&binary, &self.config_path))
            .or_else(|| Some(predicted_gpu_support(&self.config)));
    }

    fn compute_release_status(&self, worker_type: WorkerType) -> Option<WorkerReleaseStatus> {
        let runtime_bundle = manifest_runtime_bundle(&self.config, worker_type)
            .ok()
            .flatten();
        let cache_path = app_paths()
            .ok()?
            .version_cache_dir()
            .join(release_cache_name(worker_type, runtime_bundle.as_deref()));

        check_worker_release_status_with_runtime(
            self.config.worker_api_url(worker_type),
            worker_type.cli_product(),
            installed_version(&self.config, worker_type),
            runtime_bundle.as_deref(),
            &cache_path,
        )
        .ok()
    }

    fn compute_app_release_status(&self) -> Option<WorkerReleaseStatus> {
        let product = WorkerType::RssScrapper.desktop_bundle_product();
        let cache_path = app_paths()
            .ok()?
            .version_cache_dir()
            .join(release_cache_name_for_product(product));

        check_worker_release_status(
            self.config.worker_api_url(WorkerType::RssScrapper),
            product,
            APP_VERSION,
            &cache_path,
        )
        .ok()
    }

    pub(super) fn refresh_status(&mut self, worker_type: WorkerType) {
        let status_file = match app_paths() {
            Ok(paths) => paths.worker_paths(worker_type).status_file,
            Err(_) => return,
        };

        let snapshot = match fs::read(&status_file) {
            Ok(bytes) => serde_json::from_slice::<WorkerStatusSnapshot>(&bytes).ok(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(_) => None,
        };

        let now = Instant::now();
        let state = self.state_mut(worker_type);
        let observed_processing = snapshot
            .as_ref()
            .map(|snapshot| {
                matches!(snapshot.phase, WorkerPhase::Processing) || snapshot.current_task.is_some()
            })
            .unwrap_or(false);
        let observed_completed_work = match (state.status_snapshot.as_ref(), snapshot.as_ref()) {
            (Some(previous), Some(current)) => {
                current.completed_task_count > previous.completed_task_count
            }
            _ => false,
        };

        if observed_processing || observed_completed_work {
            state.note_processing_activity(now);
        }

        if snapshot
            .as_ref()
            .map(|snapshot| matches!(snapshot.phase, WorkerPhase::Error | WorkerPhase::Stopped))
            .unwrap_or(false)
        {
            state.clear_processing_hint();
        }

        state.status_snapshot = snapshot;
    }

    fn poll_child(&mut self, worker_type: WorkerType) {
        let state = self.state_mut(worker_type);
        let Some(child) = state.child.as_mut() else {
            return;
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                state.notice = Some(super::super::state::UiNotice::neutral(format!(
                    "Worker stopped ({status})."
                )));
                state.child = None;
            }
            Ok(None) => {}
            Err(error) => {
                state.notice = Some(super::super::state::UiNotice::warning(format!(
                    "Worker status is unavailable. {}",
                    summarize_detail(&error.to_string())
                )));
            }
        }
    }

    fn clear_expired_notices(&mut self) {
        let now = Instant::now();

        if self
            .global_notice
            .as_ref()
            .map(|notice| notice.is_expired(now))
            .unwrap_or(false)
        {
            self.global_notice = None;
        }

        for worker_type in ALL_WORKERS {
            let state = self.state_mut(worker_type);
            if state
                .notice
                .as_ref()
                .map(|notice| notice.is_expired(now))
                .unwrap_or(false)
            {
                state.notice = None;
            }
        }
    }

    fn reconcile_outdated_worker(&mut self, worker_type: WorkerType) {
        let requires_update =
            worker_requires_update(self.state(worker_type).release_status.as_ref());
        if !requires_update {
            self.state_mut(worker_type).awaiting_update_stop = false;
            return;
        }

        if !self.is_running(worker_type) {
            self.state_mut(worker_type).awaiting_update_stop = false;
            return;
        }

        if worker_is_busy(self.state(worker_type).status_snapshot.as_ref()) {
            let worker_name = worker_type.display_name();
            let state = self.state_mut(worker_type);
            state.awaiting_update_stop = true;
            state.notice = Some(UiNotice::warning(format!(
                "{worker_name} has an update available. Current work will finish before the worker stops."
            )));
            return;
        }

        let stop_result = self.stop_worker(worker_type);
        let worker_name = worker_type.display_name();
        let state = self.state_mut(worker_type);
        state.awaiting_update_stop = false;
        state.notice = Some(match stop_result {
            Ok(()) => UiNotice::warning(format!(
                "{worker_name} stopped because a newer bundle is available. Install the update before restarting."
            )),
            Err(error) => UiNotice::danger(error),
        });
    }
}
