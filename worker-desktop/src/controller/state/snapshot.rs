use crate::{SettingsCardView, WorkerCardView};

use super::notice::{hidden_notice_view, UiNotice, WorkerStatusTone};

#[derive(Clone, Debug)]
pub(crate) struct UiInputs {
    pub(crate) api_url: String,
    pub(crate) rss_api_key: String,
    pub(crate) rss_run_mode_index: i32,
    pub(crate) rss_max_requests: i32,
    pub(crate) embedding_api_key: String,
    pub(crate) embedding_run_mode_index: i32,
    pub(crate) embedding_acceleration_index: i32,
    pub(crate) embedding_batch_size: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkerCardSnapshot {
    pub(crate) version_line: String,
    pub(crate) status_label: String,
    pub(crate) status_tone: WorkerStatusTone,
    pub(crate) show_install_action: bool,
    pub(crate) can_install_or_update: bool,
    pub(crate) install_label: String,
    pub(crate) run_label: String,
    pub(crate) can_toggle_run: bool,
    pub(crate) can_uninstall: bool,
    pub(crate) message: Option<UiNotice>,
}

impl WorkerCardSnapshot {
    pub(crate) fn to_view(&self) -> WorkerCardView {
        WorkerCardView {
            version_line: self.version_line.clone().into(),
            status_label: self.status_label.clone().into(),
            status_tone: self.status_tone.into(),
            show_install_action: self.show_install_action,
            can_install_or_update: self.can_install_or_update,
            install_label: self.install_label.clone().into(),
            run_label: self.run_label.clone().into(),
            can_toggle_run: self.can_toggle_run,
            can_uninstall: self.can_uninstall,
            notice: self
                .message
                .as_ref()
                .map(UiNotice::to_view)
                .unwrap_or_else(hidden_notice_view),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsCardSnapshot {
    pub(crate) version_line: String,
    pub(crate) notice: Option<UiNotice>,
    pub(crate) can_open_update: bool,
    pub(crate) can_open_release_notes: bool,
}

impl SettingsCardSnapshot {
    pub(crate) fn to_view(&self) -> SettingsCardView {
        SettingsCardView {
            version_line: self.version_line.clone().into(),
            can_open_update: self.can_open_update,
            can_open_release_notes: self.can_open_release_notes,
            notice: self
                .notice
                .as_ref()
                .map(UiNotice::to_view)
                .unwrap_or_else(hidden_notice_view),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DashboardSnapshot {
    pub(crate) inputs: UiInputs,
    pub(crate) settings: SettingsCardSnapshot,
    pub(crate) app_busy: bool,
    pub(crate) app_read_only: bool,
    pub(crate) global_notice: Option<UiNotice>,
    pub(crate) rss: WorkerCardSnapshot,
    pub(crate) embedding: WorkerCardSnapshot,
}
