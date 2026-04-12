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
use super::super::utils::{predicted_gpu_support, summarize_detail};
use super::AppCore;

enum StatusLoadResult {
    Missing,
    Loaded(WorkerStatusSnapshot),
    Error {
        prefix: &'static str,
        detail: String,
    },
}

impl AppCore {
    pub(in crate::controller) fn refresh(&mut self) {
        for worker_type in ALL_WORKERS {
            self.poll_child(worker_type);
            self.refresh_status(worker_type);
        }
        self.clear_expired_notices();
    }

    pub(in crate::controller) fn refresh_release_statuses(&mut self) {
        self.app_release_status = self.compute_app_release_status();
        for worker_type in ALL_WORKERS {
            let status = self.compute_release_status(worker_type);
            self.state_mut(worker_type).release_status = status;
        }
    }

    pub(in crate::controller) fn refresh_gpu_support(&mut self) {
        if self.is_read_only() {
            self.gpu_support = Some(predicted_gpu_support(&self.config));
            return;
        }

        self.gpu_support = self
            .binary_path(WorkerType::SourceEmbedding)
            .and_then(|binary| {
                self.config_path
                    .as_deref()
                    .map(|config_path| GpuSupport::probe(&binary, config_path))
            })
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

        let result = match fs::read(&status_file) {
            Ok(bytes) => match serde_json::from_slice::<WorkerStatusSnapshot>(&bytes) {
                Ok(snapshot) => StatusLoadResult::Loaded(snapshot),
                Err(error) => StatusLoadResult::Error {
                    prefix: "Worker status file is invalid.",
                    detail: error.to_string(),
                },
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => StatusLoadResult::Missing,
            Err(error) => StatusLoadResult::Error {
                prefix: "Worker status file is unavailable.",
                detail: error.to_string(),
            },
        };

        apply_status_load_result(self.state_mut(worker_type), result, Instant::now());
    }

    fn poll_child(&mut self, worker_type: WorkerType) {
        let state = self.state_mut(worker_type);
        let Some(child) = state.child.as_mut() else {
            return;
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                state.notice = Some(UiNotice::neutral(format!("Worker stopped ({status}).")));
                state.child = None;
            }
            Ok(None) => {}
            Err(error) => {
                state.notice = Some(UiNotice::warning(format!(
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
}

fn apply_status_load_result(
    state: &mut super::super::state::WorkerRuntimeState,
    result: StatusLoadResult,
    now: Instant,
) {
    match result {
        StatusLoadResult::Missing => {
            state.status_file_notice = None;
            state.status_snapshot = None;
        }
        StatusLoadResult::Loaded(snapshot) => {
            state.status_file_notice = None;

            let observed_processing = matches!(snapshot.phase, WorkerPhase::Processing)
                || snapshot.current_task.is_some();
            let observed_completed_work = state
                .status_snapshot
                .as_ref()
                .map(|previous| snapshot.completed_task_count > previous.completed_task_count)
                .unwrap_or(false);

            if observed_processing || observed_completed_work {
                state.note_processing_activity(now);
            }

            if matches!(snapshot.phase, WorkerPhase::Error | WorkerPhase::Stopped) {
                state.clear_processing_hint();
            }

            state.status_snapshot = Some(snapshot);
        }
        StatusLoadResult::Error { prefix, detail } => {
            state.status_file_notice = Some(UiNotice::warning(format!(
                "{prefix} {}",
                summarize_detail(&detail)
            )));
        }
    }
}

#[cfg(test)]
#[path = "refresh_service_tests.rs"]
mod tests;
