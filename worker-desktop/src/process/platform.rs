use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

use manifeed_worker_common::WorkerType;

use super::identity::process_identity_matches_worker;

pub(super) fn process_exists(pid: u32) -> bool {
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

pub fn terminate_process(
    worker_type: WorkerType,
    pid: u32,
    expected_binary: Option<&Path>,
) -> Result<(), String> {
    if matches!(std::env::consts::OS, "linux" | "macos")
        && !process_matches_worker(pid, expected_binary, worker_type)
    {
        return Err(format!(
            "Refusing to stop pid {pid} because it could not be verified as {}.",
            worker_type.display_name()
        ));
    }

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

pub(super) fn linux_process_matches_worker(
    pid: u32,
    expected_binary: Option<&Path>,
    worker_type: WorkerType,
) -> bool {
    let exe_path = fs::read_link(format!("/proc/{pid}/exe")).ok();
    if process_identity_matches_worker(
        exe_path.as_deref(),
        None,
        expected_binary,
        worker_type.binary_name(),
    ) {
        return true;
    }

    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok();
    process_identity_matches_worker(
        exe_path.as_deref(),
        cmdline
            .as_deref()
            .map(|cmdline| String::from_utf8_lossy(cmdline).replace('\0', " ")),
        expected_binary,
        worker_type.binary_name(),
    )
}

pub(super) fn macos_process_matches_worker(
    pid: u32,
    expected_binary: Option<&Path>,
    worker_type: WorkerType,
) -> bool {
    let command = ProcessCommand::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());

    process_identity_matches_worker(None, command, expected_binary, worker_type.binary_name())
}

pub(super) fn process_matches_worker(
    pid: u32,
    expected_binary: Option<&Path>,
    worker_type: WorkerType,
) -> bool {
    match std::env::consts::OS {
        "linux" => linux_process_matches_worker(pid, expected_binary, worker_type),
        "macos" => macos_process_matches_worker(pid, expected_binary, worker_type),
        _ => true,
    }
}
