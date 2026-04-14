use std::sync::mpsc::Sender;

use manifeed_worker_common::WorkerType;
use slint::ComponentHandle;

use crate::WorkersDashboardWindow;

use super::state::{hidden_notice_view, Command, DashboardSnapshot, UiEdits};

pub(super) fn bind_callbacks(window: &WorkersDashboardWindow, sender: Sender<Command>) {
    let save_weak = window.as_weak();
    let save_sender = sender.clone();
    window.on_request_save_changes(move || {
        if let Some(window) = save_weak.upgrade() {
            if window.get_app_busy() || window.get_app_read_only() {
                return;
            }
            window.set_app_busy(true);
            let _ = save_sender.send(Command::SaveChanges(UiEdits::from_window(&window)));
        }
    });

    let check_updates_weak = window.as_weak();
    let check_updates_sender = sender.clone();
    window.on_request_check_updates(move || {
        if let Some(window) = check_updates_weak.upgrade() {
            if window.get_app_busy() || window.get_app_read_only() {
                return;
            }
            window.set_app_busy(true);
        }
        let _ = check_updates_sender.send(Command::CheckUpdates);
    });

    let open_download_sender = sender.clone();
    window.on_request_open_desktop_download(move || {
        let _ = open_download_sender.send(Command::OpenDesktopDownload);
    });

    let open_notes_sender = sender.clone();
    window.on_request_open_desktop_release_notes(move || {
        let _ = open_notes_sender.send(Command::OpenDesktopReleaseNotes);
    });

    bind_worker_callback(
        window,
        sender.clone(),
        WorkerType::RssScrapper,
        WorkersDashboardWindow::on_request_rss_check_api,
        Command::CheckApi,
    );
    bind_worker_callback(
        window,
        sender.clone(),
        WorkerType::RssScrapper,
        WorkersDashboardWindow::on_request_rss_install_or_update,
        Command::InstallOrUpdate,
    );
    bind_worker_callback(
        window,
        sender.clone(),
        WorkerType::RssScrapper,
        WorkersDashboardWindow::on_request_rss_toggle_run,
        Command::ToggleRun,
    );
    bind_worker_callback(
        window,
        sender.clone(),
        WorkerType::RssScrapper,
        WorkersDashboardWindow::on_request_rss_uninstall,
        Command::Uninstall,
    );
    bind_worker_callback(
        window,
        sender.clone(),
        WorkerType::SourceEmbedding,
        WorkersDashboardWindow::on_request_embedding_check_api,
        Command::CheckApi,
    );
    bind_worker_callback(
        window,
        sender.clone(),
        WorkerType::SourceEmbedding,
        WorkersDashboardWindow::on_request_embedding_install_or_update,
        Command::InstallOrUpdate,
    );
    bind_worker_callback(
        window,
        sender.clone(),
        WorkerType::SourceEmbedding,
        WorkersDashboardWindow::on_request_embedding_toggle_run,
        Command::ToggleRun,
    );
    bind_worker_callback(
        window,
        sender,
        WorkerType::SourceEmbedding,
        WorkersDashboardWindow::on_request_embedding_uninstall,
        Command::Uninstall,
    );
}

fn bind_worker_callback<BindFn, CommandFn>(
    window: &WorkersDashboardWindow,
    sender: Sender<Command>,
    worker_type: WorkerType,
    bind_fn: BindFn,
    command_fn: CommandFn,
) where
    BindFn: Fn(&WorkersDashboardWindow, Box<dyn Fn()>) + 'static,
    CommandFn: Fn(WorkerType, UiEdits) -> Command + Copy + 'static,
{
    let weak = window.as_weak();
    bind_fn(
        window,
        Box::new(move || {
            if let Some(window) = weak.upgrade() {
                if window.get_app_busy() || window.get_app_read_only() {
                    return;
                }
                window.set_app_busy(true);
                let _ = sender.send(command_fn(worker_type, UiEdits::from_window(&window)));
            }
        }),
    );
}

pub(super) fn apply_snapshot(
    window: &WorkersDashboardWindow,
    snapshot: &DashboardSnapshot,
    sync_inputs: bool,
) {
    if sync_inputs {
        window.set_api_url(snapshot.inputs.api_url.clone().into());
        window.set_rss_api_key(snapshot.inputs.rss_api_key.clone().into());
        window.set_rss_run_mode_index(snapshot.inputs.rss_run_mode_index);
        window.set_rss_max_requests(snapshot.inputs.rss_max_requests);
        window.set_embedding_api_key(snapshot.inputs.embedding_api_key.clone().into());
        window.set_embedding_run_mode_index(snapshot.inputs.embedding_run_mode_index);
        window.set_embedding_acceleration_index(snapshot.inputs.embedding_acceleration_index);
        window.set_embedding_batch_size(snapshot.inputs.embedding_batch_size);
    }

    window.set_settings_card(snapshot.settings.to_view());
    window.set_app_busy(snapshot.app_busy);
    window.set_app_read_only(snapshot.app_read_only);
    window.set_global_notice(
        snapshot
            .global_notice
            .as_ref()
            .map(|notice| notice.to_view())
            .unwrap_or_else(hidden_notice_view),
    );
    window.set_rss_card(snapshot.rss.to_view());
    window.set_embedding_card(snapshot.embedding.to_view());
}
