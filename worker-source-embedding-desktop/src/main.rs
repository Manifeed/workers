mod controller;
mod gpu;
mod installer;
mod process;
mod worker_support;

use std::time::Duration;

use controller::{Command, DesktopController};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = WorkersDashboardWindow::new()?;
    let controller = DesktopController::new(&window);
    let refresh_sender = controller.sender();
    let refresh_timer = slint::Timer::default();

    refresh_timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(300),
        move || {
            let _ = refresh_sender.send(Command::RefreshTick);
        },
    );

    controller.start();
    window.run()?;
    drop(refresh_timer);
    controller.shutdown();
    Ok(())
}
