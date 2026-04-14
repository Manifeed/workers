mod notice;
mod snapshot;

use std::process::Child;
use std::time::{Duration, Instant};

use manifeed_worker_common::{
    AccelerationMode, ServiceMode, WorkerReleaseStatus, WorkerStatusSnapshot, WorkerType,
};

use crate::WorkersDashboardWindow;

use super::utils::{acceleration_mode_from_index, service_mode_from_index};

pub(crate) use notice::{hidden_notice_view, UiNotice, WorkerStatusTone};
pub(crate) use snapshot::{DashboardSnapshot, SettingsCardSnapshot, UiInputs, WorkerCardSnapshot};

pub const APP_VERSION: &str = match option_env!("MANIFEED_DESKTOP_APP_VERSION") {
    Some(value) => value,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Clone, Debug)]
pub(crate) enum Command {
    Initialize,
    RefreshTick,
    SaveChanges(UiEdits),
    CheckUpdates,
    CheckApi(WorkerType, UiEdits),
    InstallOrUpdate(WorkerType, UiEdits),
    ToggleRun(WorkerType, UiEdits),
    Uninstall(WorkerType, UiEdits),
    OpenDesktopDownload,
    OpenDesktopReleaseNotes,
    Shutdown,
}

#[derive(Clone, Debug)]
pub(crate) struct UiEdits {
    pub(super) api_url: String,
    pub(super) rss_api_key: String,
    pub(super) rss_run_mode: ServiceMode,
    pub(super) rss_max_requests: usize,
    pub(super) embedding_api_key: String,
    pub(super) embedding_run_mode: ServiceMode,
    pub(super) embedding_acceleration_mode: AccelerationMode,
    pub(super) embedding_batch_size: usize,
}

impl UiEdits {
    pub(super) fn from_window(window: &WorkersDashboardWindow) -> Self {
        Self {
            api_url: window.get_api_url().to_string(),
            rss_api_key: window.get_rss_api_key().to_string(),
            rss_run_mode: service_mode_from_index(window.get_rss_run_mode_index()),
            rss_max_requests: window.get_rss_max_requests().max(1) as usize,
            embedding_api_key: window.get_embedding_api_key().to_string(),
            embedding_run_mode: service_mode_from_index(window.get_embedding_run_mode_index()),
            embedding_acceleration_mode: acceleration_mode_from_index(
                window.get_embedding_acceleration_index(),
            ),
            embedding_batch_size: window.get_embedding_batch_size().max(1) as usize,
        }
    }
}

#[derive(Default)]
pub(super) struct WorkerRuntimeState {
    pub(super) child: Option<Child>,
    pub(super) status_snapshot: Option<WorkerStatusSnapshot>,
    pub(super) release_status: Option<WorkerReleaseStatus>,
    pub(super) notice: Option<UiNotice>,
    pub(super) status_file_notice: Option<UiNotice>,
    pub(super) processing_hint_until: Option<Instant>,
}

const PROCESSING_HINT_DURATION: Duration = Duration::from_secs(2);

impl WorkerRuntimeState {
    pub(super) fn note_processing_activity(&mut self, now: Instant) {
        self.processing_hint_until = Some(now + PROCESSING_HINT_DURATION);
    }

    pub(super) fn clear_processing_hint(&mut self) {
        self.processing_hint_until = None;
    }

    pub(super) fn recent_processing(&self, now: Instant) -> bool {
        self.processing_hint_until
            .map(|until| now < until)
            .unwrap_or(false)
    }
}
