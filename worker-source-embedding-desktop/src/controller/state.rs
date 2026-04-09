use std::process::Child;
use std::time::{Duration, Instant};

use manifeed_worker_common::{
    AccelerationMode, ServiceMode, WorkerReleaseStatus, WorkerStatusSnapshot, WorkerType,
};

use crate::{NoticeView, SettingsCardView, WorkerCardView, WorkersDashboardWindow};

use super::utils::{acceleration_mode_from_index, service_mode_from_index};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Tone {
    Neutral,
    Success,
    Warning,
    Danger,
}

impl Tone {
    fn code(self) -> i32 {
        match self {
            Self::Neutral => 0,
            Self::Success => 1,
            Self::Warning => 2,
            Self::Danger => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct UiNotice {
    tone: Tone,
    text: String,
    expires_at: Option<Instant>,
}

impl UiNotice {
    pub(super) fn neutral(text: impl Into<String>) -> Self {
        Self::transient(Tone::Neutral, text)
    }

    pub(super) fn success(text: impl Into<String>) -> Self {
        Self::transient(Tone::Success, text)
    }

    pub(super) fn warning(text: impl Into<String>) -> Self {
        Self::persistent(Tone::Warning, text)
    }

    pub(super) fn danger(text: impl Into<String>) -> Self {
        Self::persistent(Tone::Danger, text)
    }

    pub(super) fn neutral_persistent(text: impl Into<String>) -> Self {
        Self::persistent(Tone::Neutral, text)
    }

    fn transient(tone: Tone, text: impl Into<String>) -> Self {
        Self {
            tone,
            text: text.into(),
            expires_at: Some(Instant::now() + Duration::from_secs(5)),
        }
    }

    fn persistent(tone: Tone, text: impl Into<String>) -> Self {
        Self {
            tone,
            text: text.into(),
            expires_at: None,
        }
    }

    pub(super) fn is_expired(&self, now: Instant) -> bool {
        self.expires_at
            .map(|expires_at| now >= expires_at)
            .unwrap_or(false)
    }

    pub(super) fn priority(&self) -> u8 {
        match self.tone {
            Tone::Danger => 4,
            Tone::Warning => 3,
            Tone::Neutral => 2,
            Tone::Success => 1,
        }
    }

    pub(super) fn to_view(&self) -> NoticeView {
        NoticeView {
            visible: true,
            text: self.text.clone().into(),
            tone: self.tone.code(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkerStatusTone {
    Inactive,
    Active,
    Processing,
    Error,
}

impl WorkerStatusTone {
    fn code(self) -> i32 {
        match self {
            Self::Inactive => 0,
            Self::Active => 1,
            Self::Processing => 2,
            Self::Error => 3,
        }
    }
}

pub(super) fn hidden_notice_view() -> NoticeView {
    NoticeView {
        visible: false,
        text: "".into(),
        tone: Tone::Neutral.code(),
    }
}

#[derive(Clone, Debug)]
pub(super) struct UiInputs {
    pub(super) api_url: String,
    pub(super) rss_api_key: String,
    pub(super) rss_run_mode_index: i32,
    pub(super) rss_max_requests: i32,
    pub(super) embedding_api_key: String,
    pub(super) embedding_run_mode_index: i32,
    pub(super) embedding_acceleration_index: i32,
    pub(super) embedding_batch_size: i32,
}

#[derive(Clone, Debug)]
pub(super) struct WorkerCardSnapshot {
    pub(super) version_line: String,
    pub(super) status_label: String,
    pub(super) status_tone: WorkerStatusTone,
    pub(super) show_install_action: bool,
    pub(super) install_label: String,
    pub(super) run_label: String,
    pub(super) can_toggle_run: bool,
    pub(super) can_uninstall: bool,
    pub(super) busy: bool,
    pub(super) message: Option<UiNotice>,
}

impl WorkerCardSnapshot {
    pub(super) fn to_view(&self) -> WorkerCardView {
        WorkerCardView {
            version_line: self.version_line.clone().into(),
            status_label: self.status_label.clone().into(),
            status_tone: self.status_tone.code(),
            show_install_action: self.show_install_action,
            install_label: self.install_label.clone().into(),
            run_label: self.run_label.clone().into(),
            can_toggle_run: self.can_toggle_run,
            can_uninstall: self.can_uninstall,
            busy: self.busy,
            notice: self
                .message
                .as_ref()
                .map(UiNotice::to_view)
                .unwrap_or_else(hidden_notice_view),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SettingsCardSnapshot {
    pub(super) version_line: String,
    pub(super) notice: Option<UiNotice>,
    pub(super) can_open_update: bool,
    pub(super) can_open_release_notes: bool,
}

impl SettingsCardSnapshot {
    pub(super) fn to_view(&self) -> SettingsCardView {
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
pub(super) struct DashboardSnapshot {
    pub(super) inputs: UiInputs,
    pub(super) settings: SettingsCardSnapshot,
    pub(super) app_busy: bool,
    pub(super) global_notice: Option<UiNotice>,
    pub(super) rss: WorkerCardSnapshot,
    pub(super) embedding: WorkerCardSnapshot,
}

#[derive(Default)]
pub(super) struct WorkerRuntimeState {
    pub(super) child: Option<Child>,
    pub(super) status_snapshot: Option<WorkerStatusSnapshot>,
    pub(super) release_status: Option<WorkerReleaseStatus>,
    pub(super) notice: Option<UiNotice>,
    pub(super) awaiting_update_stop: bool,
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
