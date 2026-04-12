use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::ComponentHandle;

use crate::WorkersDashboardWindow;

use super::bindings::{apply_snapshot, bind_callbacks};
use super::core::AppCore;
use super::state::Command;

pub(crate) struct DesktopController {
    sender: Sender<Command>,
    worker_thread: Option<JoinHandle<()>>,
    refresh_pending: Arc<AtomicBool>,
}

#[derive(Clone)]
pub(crate) struct RefreshTicker {
    sender: Sender<Command>,
    refresh_pending: Arc<AtomicBool>,
}

impl RefreshTicker {
    pub(crate) fn schedule(&self) {
        if self
            .refresh_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        if self.sender.send(Command::RefreshTick).is_err() {
            self.refresh_pending.store(false, Ordering::Release);
        }
    }
}

impl DesktopController {
    pub(crate) fn new(window: &WorkersDashboardWindow) -> Self {
        let ui = window.as_weak();
        let (sender, receiver) = mpsc::channel();
        let refresh_pending = Arc::new(AtomicBool::new(false));

        bind_callbacks(window, sender.clone());

        let runtime_refresh_pending = Arc::clone(&refresh_pending);
        let worker_thread = thread::spawn(move || {
            let mut runtime = AppRuntime::bootstrap(ui, runtime_refresh_pending);
            runtime.run(receiver);
        });

        Self {
            sender,
            worker_thread: Some(worker_thread),
            refresh_pending,
        }
    }

    pub(crate) fn refresh_ticker(&self) -> RefreshTicker {
        RefreshTicker {
            sender: self.sender.clone(),
            refresh_pending: Arc::clone(&self.refresh_pending),
        }
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
    refresh_pending: Arc<AtomicBool>,
}

impl AppRuntime {
    fn bootstrap(
        ui: slint::Weak<WorkersDashboardWindow>,
        refresh_pending: Arc<AtomicBool>,
    ) -> Self {
        Self {
            ui,
            core: AppCore::bootstrap(),
            refresh_pending,
        }
    }

    fn run(&mut self, receiver: Receiver<Command>) {
        let mut queued_command = None;

        loop {
            let command = match queued_command.take() {
                Some(command) => command,
                None => match receiver.recv() {
                    Ok(command) => command,
                    Err(_) => break,
                },
            };

            match command {
                Command::Shutdown => {
                    self.core.stop_all_children();
                    break;
                }
                Command::RefreshTick => {
                    self.refresh_pending.store(false, Ordering::Release);
                    self.handle_command(Command::RefreshTick);

                    loop {
                        match receiver.try_recv() {
                            Ok(Command::RefreshTick) => {
                                self.refresh_pending.store(false, Ordering::Release);
                            }
                            Ok(Command::Shutdown) => {
                                self.core.stop_all_children();
                                return;
                            }
                            Ok(other) => {
                                queued_command = Some(other);
                                break;
                            }
                            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                        }
                    }
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
                self.handle_mutating_command(
                    true,
                    |core| core.begin_save(),
                    move |core| core.save_changes(edits),
                );
            }
            Command::CheckUpdates => {
                self.handle_mutating_command(
                    true,
                    |core| core.begin_update_check(),
                    |core| core.check_updates(),
                );
            }
            Command::CheckApi(worker_type, edits) => {
                self.handle_mutating_command(
                    true,
                    move |core| core.begin_worker_action(worker_type, "Checking API..."),
                    move |core| core.test_connection(worker_type, edits),
                );
            }
            Command::InstallOrUpdate(worker_type, edits) => {
                self.handle_mutating_command(
                    true,
                    move |core| core.begin_worker_action(worker_type, "Preparing installation..."),
                    move |core| core.install_or_update(worker_type, edits),
                );
            }
            Command::ToggleRun(worker_type, edits) => {
                self.handle_mutating_command(
                    true,
                    move |core| {
                        let verb = if core.is_running(worker_type) {
                            "Stopping worker..."
                        } else {
                            "Starting worker..."
                        };
                        core.begin_worker_action(worker_type, verb);
                    },
                    move |core| core.toggle_run(worker_type, edits),
                );
            }
            Command::Uninstall(worker_type, edits) => {
                self.handle_mutating_command(
                    true,
                    move |core| core.begin_worker_action(worker_type, "Removing bundle..."),
                    move |core| core.uninstall(worker_type, edits),
                );
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

    fn handle_mutating_command<Prepare, Execute>(
        &mut self,
        sync_inputs_on_complete: bool,
        prepare: Prepare,
        execute: Execute,
    ) where
        Prepare: FnOnce(&mut AppCore),
        Execute: FnOnce(&mut AppCore),
    {
        if self.core.is_busy() {
            return;
        }

        if self.core.require_writable().is_err() {
            self.publish(true);
            return;
        }

        prepare(&mut self.core);
        self.publish(false);
        execute(&mut self.core);
        self.core.end_action();
        self.publish(sync_inputs_on_complete);
    }

    fn publish(&self, sync_inputs: bool) {
        let snapshot = self.core.snapshot();
        let ui = self.ui.clone();
        let _ = ui.upgrade_in_event_loop(move |window| {
            apply_snapshot(&window, &snapshot, sync_inputs);
        });
    }
}
