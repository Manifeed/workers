use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use manifeed_worker_common::WorkerType;
use slint::ComponentHandle;

use crate::WorkersDashboardWindow;

use super::core::AppCore;
use super::state::{hidden_notice_view, Command, DashboardSnapshot, UiEdits};

pub(crate) struct DesktopController {
    sender: Sender<Command>,
    worker_thread: Option<JoinHandle<()>>,
}

impl DesktopController {
    pub(crate) fn new(window: &WorkersDashboardWindow) -> Self {
        let ui = window.as_weak();
        let (sender, receiver) = mpsc::channel();

        bind_callbacks(window, sender.clone());

        let worker_thread = thread::spawn(move || {
            let mut runtime = AppRuntime::bootstrap(ui);
            runtime.run(receiver);
        });

        Self {
            sender,
            worker_thread: Some(worker_thread),
        }
    }

    pub(crate) fn sender(&self) -> Sender<Command> {
        self.sender.clone()
    }

    pub(crate) fn start(&self) {
        let _ = self.sender.send(Command::Initialize);
    }

    pub(crate) fn shutdown(mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Some(handle) = self.worker_thread.take() {
            let _ = handle.join();
        }
    }
}

struct AppRuntime {
    ui: slint::Weak<WorkersDashboardWindow>,
    core: AppCore,
}

impl AppRuntime {
    fn bootstrap(ui: slint::Weak<WorkersDashboardWindow>) -> Self {
        Self {
            ui,
            core: AppCore::bootstrap(),
        }
    }

    fn run(&mut self, receiver: Receiver<Command>) {
        while let Ok(command) = receiver.recv() {
            match command {
                Command::Shutdown => {
                    self.core.stop_all_children();
                    break;
                }
                other => self.handle_command(other),
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Initialize => {
                self.publish(true);
                self.core.refresh_release_statuses();
                self.core.refresh_gpu_support();
                self.publish(true);
            }
            Command::RefreshTick => {
                self.core.refresh();
                self.publish(false);
            }
            Command::SaveChanges(edits) => {
                self.publish(false);
                self.core.save_changes(edits);
                self.publish(true);
            }
            Command::CheckUpdates => {
                self.publish(false);
                self.core.check_updates();
                self.publish(true);
            }
            Command::CheckApi(worker_type, edits) => {
                self.publish(false);
                self.core.test_connection(worker_type, edits);
                self.publish(true);
            }
            Command::InstallOrUpdate(worker_type, edits) => {
                self.publish(false);
                self.core.install_or_update(worker_type, edits);
                self.publish(true);
            }
            Command::ToggleRun(worker_type, edits) => {
                self.publish(false);
                self.core.toggle_run(worker_type, edits);
                self.publish(true);
            }
            Command::Uninstall(worker_type, edits) => {
                self.publish(false);
                self.core.uninstall(worker_type, edits);
                self.publish(true);
            }
            Command::OpenDesktopDownload => {
                self.core.open_desktop_download();
                self.publish(false);
            }
            Command::OpenDesktopReleaseNotes => {
                self.core.open_desktop_release_notes();
                self.publish(false);
            }
            Command::Shutdown => {}
        }
    }

    fn publish(&self, sync_inputs: bool) {
        let snapshot = self.core.snapshot();
        let ui = self.ui.clone();
        let _ = ui.upgrade_in_event_loop(move |window| {
            apply_snapshot(&window, &snapshot, sync_inputs);
        });
    }
}

fn bind_callbacks(window: &WorkersDashboardWindow, sender: Sender<Command>) {
    let save_weak = window.as_weak();
    let save_sender = sender.clone();
    window.on_request_save_changes(move || {
        if let Some(window) = save_weak.upgrade() {
            let _ = save_sender.send(Command::SaveChanges(UiEdits::from_window(&window)));
        }
    });

    let check_updates_sender = sender.clone();
    window.on_request_check_updates(move || {
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
                let _ = sender.send(command_fn(worker_type, UiEdits::from_window(&window)));
            }
        }),
    );
}

fn apply_snapshot(
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
