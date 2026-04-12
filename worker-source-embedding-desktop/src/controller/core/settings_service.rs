use manifeed_worker_common::{
    check_worker_connection, install_user_service, load_workers_config, save_workers_config,
    uninstall_user_service, ReleaseCheckStatus, ServiceMode, WorkerType, WorkersConfig,
};

use crate::process::open_external_url;
use crate::worker_support::{api_credentials, service_mode};

use super::super::state::{UiEdits, UiNotice};
use super::super::utils::{
    connection_error_notice, connection_failure_notice, normalize_api_url, planned_service_sync,
    summarize_detail, ServiceSyncAction,
};
use super::AppCore;

impl AppCore {
    pub(in crate::controller) fn open_desktop_download(&mut self) {
        self.open_desktop_release_url(|manifest| &manifest.download_url, "download");
    }

    pub(in crate::controller) fn open_desktop_release_notes(&mut self) {
        self.open_desktop_release_url(|manifest| &manifest.release_notes_url, "release notes");
    }

    pub(in crate::controller) fn check_updates(&mut self) {
        self.refresh_release_statuses();
        self.refresh_gpu_support();

        let release_check_failed = self
            .app_release_status
            .as_ref()
            .map(|status| status.status == ReleaseCheckStatus::Unverified)
            .unwrap_or(true)
            || [WorkerType::RssScrapper, WorkerType::SourceEmbedding]
                .into_iter()
                .any(|worker_type| {
                    self.state(worker_type)
                        .release_status
                        .as_ref()
                        .map(|status| status.status == ReleaseCheckStatus::Unverified)
                        .unwrap_or(true)
                });

        self.global_notice = Some(if release_check_failed {
            UiNotice::warning("Update check is unavailable. Verify the API URL and try again.")
        } else {
            UiNotice::success("Update information refreshed.")
        });
    }

    pub(in crate::controller) fn save_changes(&mut self, edits: UiEdits) {
        match self.commit_ui_edits(&edits) {
            Ok(()) => {
                self.refresh_release_statuses();
                self.refresh_gpu_support();
                self.global_notice = Some(UiNotice::success("Changes saved."));
            }
            Err(error) => {
                self.global_notice = Some(UiNotice::danger(error));
            }
        }
    }

    pub(in crate::controller) fn test_connection(
        &mut self,
        worker_type: WorkerType,
        edits: UiEdits,
    ) {
        if let Err(error) = self.commit_ui_edits(&edits) {
            self.state_mut(worker_type).notice = Some(UiNotice::danger(error));
            return;
        }

        let (url, key) = api_credentials(&self.config, worker_type);
        let notice = match check_worker_connection(url, key) {
            Ok(result) if result.ok => UiNotice::success("API is reachable."),
            Ok(result) => UiNotice::danger(connection_failure_notice(
                result.status_code,
                result.error.as_deref(),
            )),
            Err(error) => UiNotice::danger(connection_error_notice(&error)),
        };
        self.state_mut(worker_type).notice = Some(notice);
    }

    pub(super) fn commit_ui_edits(&mut self, edits: &UiEdits) -> Result<(), String> {
        let previous = self.config.clone();
        let updated = self.build_updated_config(edits);
        self.persist_updated_config(updated)?;

        if let Err(error) = self.sync_service_mode_changes(&previous) {
            return self.rollback_config(previous, error);
        }

        Ok(())
    }

    pub(super) fn commit_ui_edits_for_uninstall(&mut self, edits: &UiEdits) -> Result<(), String> {
        let updated = self.build_updated_config(edits);
        self.persist_updated_config(updated)
    }

    fn build_updated_config(&self, edits: &UiEdits) -> WorkersConfig {
        let mut updated = self.config.clone();
        updated.api_url = normalize_api_url(&edits.api_url);
        updated.rss.api_key = edits.rss_api_key.trim().to_string();
        updated.rss.service_mode = edits.rss_run_mode;
        updated.rss.max_in_flight_requests = edits.rss_max_requests.max(1);
        updated.embedding.api_key = edits.embedding_api_key.trim().to_string();
        updated.embedding.service_mode = edits.embedding_run_mode;
        updated.embedding.acceleration_mode = edits.embedding_acceleration_mode;
        updated.embedding.inference_batch_size = edits.embedding_batch_size.max(1);
        updated
    }

    fn persist_updated_config(&mut self, updated: WorkersConfig) -> Result<(), String> {
        save_workers_config(self.config_path()?, &updated).map_err(|error| {
            format!(
                "Could not save changes. {}",
                summarize_detail(&error.to_string())
            )
        })?;
        self.config = updated;
        Ok(())
    }

    fn rollback_config(
        &mut self,
        previous: WorkersConfig,
        primary_error: String,
    ) -> Result<(), String> {
        match save_workers_config(self.config_path()?, &previous) {
            Ok(()) => {
                self.config = previous;
                Err(primary_error)
            }
            Err(rollback_error) => {
                if let Some(config_path) = self.config_path.clone() {
                    if let Ok((_, reloaded)) = load_workers_config(Some(&config_path)) {
                        self.config = reloaded;
                    }
                }
                Err(format!(
                    "{primary_error} Rollback failed. {}",
                    summarize_detail(&rollback_error.to_string())
                ))
            }
        }
    }

    fn sync_service_mode_changes(&mut self, previous: &WorkersConfig) -> Result<(), String> {
        self.apply_service_mode_change(
            WorkerType::RssScrapper,
            service_mode(previous, WorkerType::RssScrapper),
        )?;
        self.apply_service_mode_change(
            WorkerType::SourceEmbedding,
            service_mode(previous, WorkerType::SourceEmbedding),
        )?;
        Ok(())
    }

    fn apply_service_mode_change(
        &mut self,
        worker_type: WorkerType,
        previous_mode: ServiceMode,
    ) -> Result<(), String> {
        let action = planned_service_sync(
            previous_mode,
            service_mode(&self.config, worker_type),
            self.is_installed(worker_type),
        );

        match action {
            None => Ok(()),
            Some(ServiceSyncAction::InstallBackgroundService) => {
                let Some(binary) = self.binary_path(worker_type) else {
                    return Err(format!(
                        "Could not enable background mode. {} is not installed.",
                        worker_type.display_name()
                    ));
                };
                install_user_service(worker_type, &binary, self.config_path()?).map_err(
                    |error| {
                        format!(
                            "Could not enable background mode. {}",
                            summarize_detail(&error.to_string())
                        )
                    },
                )?;
                self.state_mut(worker_type).notice =
                    Some(UiNotice::success("Background mode is ready."));
                Ok(())
            }
            Some(ServiceSyncAction::RemoveBackgroundService) => {
                uninstall_user_service(worker_type).map_err(|error| {
                    format!(
                        "Could not switch back to on-demand mode. {}",
                        summarize_detail(&error.to_string())
                    )
                })?;
                self.state_mut(worker_type).notice =
                    Some(UiNotice::success("On-demand mode is ready."));
                Ok(())
            }
        }
    }

    fn open_desktop_release_url<F>(&mut self, url_fn: F, label: &str)
    where
        F: Fn(&manifeed_worker_common::WorkerReleaseManifest) -> &str,
    {
        let Some(manifest) = self
            .app_release_status
            .as_ref()
            .and_then(|status| status.manifest.as_ref())
        else {
            self.global_notice = Some(UiNotice::danger(format!("Desktop {label} is unavailable.")));
            return;
        };

        let url = url_fn(manifest).trim();
        if url.is_empty() {
            self.global_notice = Some(UiNotice::danger(format!("Desktop {label} is unavailable.")));
            return;
        }

        match open_external_url(url) {
            Ok(()) => {
                self.global_notice = Some(UiNotice::neutral(format!("Opening desktop {label}...")));
            }
            Err(error) => {
                self.global_notice = Some(UiNotice::danger(format!(
                    "Could not open desktop {label}. {}",
                    summarize_detail(&error)
                )));
            }
        }
    }
}
