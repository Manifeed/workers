use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};

use chrono::{Duration as ChronoDuration, Utc};
use manifeed_worker_common::{WorkerPhase, WorkerStatusSnapshot, WorkerType};

const STATUS_STALE_AFTER_SECONDS: i64 = 120;

pub fn worker_status_is_stale(snapshot: &WorkerStatusSnapshot) -> bool {
    snapshot.last_updated_at < Utc::now() - ChronoDuration::seconds(STATUS_STALE_AFTER_SECONDS)
}

pub fn external_worker_running(
    worker_type: WorkerType,
    expected_binary: Option<&Path>,
    snapshot: Option<&WorkerStatusSnapshot>,
) -> bool {
    let Some(snapshot) = snapshot else {
        return false;
    };

    if worker_status_is_stale(snapshot) || matches!(snapshot.phase, WorkerPhase::Stopped) {
        return false;
    }

    if !process_exists(snapshot.pid) {
        return false;
    }

    match std::env::consts::OS {
        "linux" => linux_process_matches_worker(snapshot.pid, expected_binary, worker_type),
        "macos" => macos_process_matches_worker(snapshot.pid, expected_binary, worker_type),
        _ => true,
    }
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

fn linux_process_matches_worker(
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

fn macos_process_matches_worker(
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

fn process_matches_worker(
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

fn process_identity_matches_worker(
    exe_path: Option<&Path>,
    command_line: Option<String>,
    expected_binary: Option<&Path>,
    binary_name: &str,
) -> bool {
    if let Some(exe_path) = exe_path {
        if exe_path.file_name().and_then(|name| name.to_str()) == Some(binary_name) {
            return true;
        }

        if let Some(expected_binary) = expected_binary {
            if exe_path == expected_binary {
                return true;
            }
        }
    }

    let Some(command_line) = command_line else {
        return false;
    };

    if let Some(expected_binary) = expected_binary {
        let expected_binary = expected_binary.to_string_lossy();
        if command_line.contains(expected_binary.as_ref()) {
            return true;
        }
    }

    command_line.split_whitespace().any(|argument| {
        Path::new(argument)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(binary_name)
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::{Duration as ChronoDuration, Utc};
    use manifeed_worker_common::{
        NetworkTotalsSnapshot, ServerConnectionState, WorkerPhase, WorkerStatusSnapshot,
    };

    use super::{process_identity_matches_worker, worker_status_is_stale};

    #[test]
    fn stale_worker_status_is_rejected() {
        let snapshot = sample_snapshot(Utc::now() - ChronoDuration::seconds(180));
        assert!(worker_status_is_stale(&snapshot));
    }

    #[test]
    fn linux_identity_match_accepts_expected_binary_name_in_cmdline() {
        assert!(process_identity_matches_worker(
            None,
            Some("/usr/bin/python worker-source-embedding".to_string()),
            None,
            "worker-source-embedding",
        ));
    }

    #[test]
    fn linux_identity_match_rejects_unrelated_process() {
        assert!(!process_identity_matches_worker(
            None,
            Some("/usr/bin/python app.py".to_string()),
            None,
            "worker-source-embedding",
        ));
    }

    #[test]
    fn identity_match_accepts_expected_binary_path_in_command_line() {
        assert!(process_identity_matches_worker(
            None,
            Some(
                "/opt/manifeed/current/bin/worker-source-embedding run --config /tmp/workers.json"
                    .to_string()
            ),
            Some(Path::new(
                "/opt/manifeed/current/bin/worker-source-embedding"
            )),
            "worker-source-embedding",
        ));
    }

    fn sample_snapshot(last_updated_at: chrono::DateTime<chrono::Utc>) -> WorkerStatusSnapshot {
        WorkerStatusSnapshot {
            app_version: "0.1.0".to_string(),
            worker_type: "source_embedding".to_string(),
            acceleration_mode: None,
            execution_backend: None,
            pid: 42,
            phase: WorkerPhase::Idle,
            server_connection: ServerConnectionState::Connected,
            started_at: last_updated_at,
            last_updated_at,
            last_server_contact_at: None,
            current_task: None,
            current_feed_id: None,
            current_feed_url: None,
            completed_task_count: 0,
            network_totals: NetworkTotalsSnapshot::default(),
            last_error: None,
        }
    }
}
