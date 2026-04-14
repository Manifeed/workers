use std::time::Instant;

use manifeed_worker_common::{ReleaseCheckStatus, WorkerReleaseStatus, WorkerType};

use crate::worker_support::installed_version;

use super::super::state::{
    DashboardSnapshot, SettingsCardSnapshot, UiInputs, UiNotice, WorkerCardSnapshot,
};
use super::super::utils::{
    acceleration_mode_index, service_mode_index, worker_requires_update, worker_status_notice,
    worker_visual_status,
};
use super::AppCore;

impl AppCore {
    pub(in crate::controller) fn snapshot(&self) -> DashboardSnapshot {
        let now = Instant::now();

        DashboardSnapshot {
            inputs: UiInputs {
                api_url: self
                    .config
                    .worker_api_url(WorkerType::RssScrapper)
                    .to_string(),
                rss_api_key: self.config.rss.api_key.clone(),
                rss_run_mode_index: service_mode_index(self.config.rss.service_mode),
                rss_max_requests: self.config.rss.max_in_flight_requests as i32,
                embedding_api_key: self.config.embedding.api_key.clone(),
                embedding_run_mode_index: service_mode_index(self.config.embedding.service_mode),
                embedding_acceleration_index: acceleration_mode_index(
                    self.config.embedding.acceleration_mode,
                ),
                embedding_batch_size: self.config.embedding.inference_batch_size as i32,
            },
            settings: self.settings_snapshot(),
            app_busy: self.app_busy,
            app_read_only: self.is_read_only(),
            global_notice: self.global_notice.clone(),
            rss: self.worker_snapshot(WorkerType::RssScrapper, now),
            embedding: self.worker_snapshot(WorkerType::SourceEmbedding, now),
        }
    }

    fn settings_snapshot(&self) -> SettingsCardSnapshot {
        let latest_version = self
            .app_release_status
            .as_ref()
            .and_then(|status| status.manifest.as_ref())
            .map(|manifest| manifest.latest_version.clone());

        let version_line = match latest_version {
            Some(latest) => format!("Desktop {APP_VERSION} · Latest {latest}"),
            None => format!("Desktop {APP_VERSION}"),
        };

        let update_available = self
            .app_release_status
            .as_ref()
            .map(|status| {
                matches!(
                    status.status,
                    ReleaseCheckStatus::UpdateAvailable | ReleaseCheckStatus::Incompatible
                )
            })
            .unwrap_or(false);

        SettingsCardSnapshot {
            version_line,
            notice: app_release_notice(self.app_release_status.as_ref()),
            can_open_update: update_available
                && self
                    .app_release_status
                    .as_ref()
                    .and_then(|status| status.manifest.as_ref())
                    .is_some(),
            can_open_release_notes: update_available
                && self
                    .app_release_status
                    .as_ref()
                    .and_then(|status| status.manifest.as_ref())
                    .is_some(),
        }
    }

    fn worker_snapshot(&self, worker_type: WorkerType, now: Instant) -> WorkerCardSnapshot {
        let state = self.state(worker_type);
        let installed = self.is_installed(worker_type);
        let running = self.is_running(worker_type);
        let (status_tone, status_label) = worker_visual_status(
            state.status_snapshot.as_ref(),
            running,
            state.recent_processing(now),
        );
        let latest_version = state
            .release_status
            .as_ref()
            .and_then(|release| release.manifest.as_ref())
            .map(|manifest| manifest.latest_version.clone());
        let current_version = installed_version(&self.config, worker_type).to_string();

        let version_line = if installed {
            match latest_version {
                Some(latest) => format!("Installed {current_version} · Latest {latest}"),
                None => format!("Installed {current_version}"),
            }
        } else {
            match latest_version {
                Some(latest) => format!("Not installed · Latest {latest}"),
                None => "Not installed".to_string(),
            }
        };

        let can_toggle_run = if running {
            true
        } else {
            installed && !worker_requires_update(state.release_status.as_ref())
        };

        WorkerCardSnapshot {
            version_line,
            status_label: status_label.to_string(),
            status_tone,
            show_install_action: !installed
                || worker_requires_update(state.release_status.as_ref()),
            can_install_or_update: !running
                && (!installed || worker_requires_update(state.release_status.as_ref())),
            install_label: if installed {
                "Update".to_string()
            } else {
                "Install".to_string()
            },
            run_label: if running {
                "Stop".to_string()
            } else {
                "Start".to_string()
            },
            can_toggle_run,
            can_uninstall: installed && !running,
            message: choose_notices([
                state.notice.clone(),
                state.status_file_notice.clone(),
                worker_status_notice(state.status_snapshot.as_ref()),
                worker_release_notice(worker_type, state.release_status.as_ref(), running),
            ]),
        }
    }
}

use super::super::state::APP_VERSION;

fn app_release_notice(status: Option<&WorkerReleaseStatus>) -> Option<UiNotice> {
    match status?.status {
        ReleaseCheckStatus::UpdateAvailable => Some(UiNotice::warning(
            "A desktop update is available. Open the download or release notes.",
        )),
        ReleaseCheckStatus::Incompatible => Some(UiNotice::danger(
            "This desktop version is no longer supported. Update it before continuing.",
        )),
        _ => None,
    }
}

fn worker_release_notice(
    worker_type: WorkerType,
    status: Option<&WorkerReleaseStatus>,
    running: bool,
) -> Option<UiNotice> {
    let status = status?;
    match status.status {
        ReleaseCheckStatus::UpdateAvailable if running => Some(UiNotice::warning(format!(
            "{} has an update available. Stop it before installing the latest bundle.",
            worker_type.display_name()
        ))),
        ReleaseCheckStatus::UpdateAvailable => Some(UiNotice::warning(format!(
            "{} has an update available. Install the latest bundle before starting it.",
            worker_type.display_name()
        ))),
        ReleaseCheckStatus::Incompatible => Some(UiNotice::danger(format!(
            "{} must be updated before it can start.",
            worker_type.display_name()
        ))),
        _ => None,
    }
}

fn choose_notices<const N: usize>(notices: [Option<UiNotice>; N]) -> Option<UiNotice> {
    let mut selected = None;
    for notice in notices.into_iter().flatten() {
        let replace = selected
            .as_ref()
            .map(|current: &UiNotice| notice.priority() > current.priority())
            .unwrap_or(true);
        if replace {
            selected = Some(notice);
        }
    }
    selected
}

#[cfg(test)]
#[path = "snapshot_service_tests.rs"]
mod tests;
