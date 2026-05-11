use std::sync::atomic::{AtomicBool, Ordering};

static STDOUT_LOGS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable_stdout_logs() {
    STDOUT_LOGS_ENABLED.store(true, Ordering::Relaxed);
}

pub fn stdout_logs_enabled() -> bool {
    STDOUT_LOGS_ENABLED.load(Ordering::Relaxed)
}

pub fn stdout_log(message: impl AsRef<str>) {
    if stdout_logs_enabled() {
        println!("{}", message.as_ref());
    }
}
