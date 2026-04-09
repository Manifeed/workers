use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};

use manifeed_worker_common::{WorkerPhase, WorkerStatusSnapshot};

pub fn external_worker_running(snapshot: Option<&WorkerStatusSnapshot>) -> bool {
    snapshot
        .map(|snapshot| {
            process_exists(snapshot.pid) && !matches!(snapshot.phase.clone(), WorkerPhase::Stopped)
        })
        .unwrap_or(false)
}

fn process_exists(pid: u32) -> bool {
    match std::env::consts::OS {
        "windows" => false,
        "linux" => PathBuf::from(format!("/proc/{pid}")).exists(),
        _ => ProcessCommand::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or_else(|_| PathBuf::from(format!("/proc/{pid}")).exists()),
    }
}

pub fn terminate_process(pid: u32) -> Result<(), String> {
    match std::env::consts::OS {
        "windows" => {
            let status = ProcessCommand::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .status()
                .map_err(|error| error.to_string())?;
            if status.success() || !process_exists(pid) {
                return Ok(());
            }
            Err(format!("Could not stop process {pid} ({status})."))
        }
        _ => {
            let status = ProcessCommand::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status()
                .map_err(|error| error.to_string())?;
            if status.success() || !process_exists(pid) {
                return Ok(());
            }

            let status = ProcessCommand::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status()
                .map_err(|error| error.to_string())?;
            if status.success() || !process_exists(pid) {
                return Ok(());
            }

            Err(format!("Could not stop process {pid} ({status})."))
        }
    }
}

pub fn open_external_url(url: &str) -> Result<(), String> {
    let status = match std::env::consts::OS {
        "macos" => ProcessCommand::new("open").arg(url).status(),
        "windows" => ProcessCommand::new("cmd")
            .args(["/C", "start", "", url])
            .status(),
        _ => ProcessCommand::new("xdg-open").arg(url).status(),
    }
    .map_err(|error| error.to_string())?;

    if status.success() {
        return Ok(());
    }

    Err(format!("Could not open {url} ({status})."))
}
