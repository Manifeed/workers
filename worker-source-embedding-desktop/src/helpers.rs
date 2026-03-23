use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Local, Utc};
use manifeed_worker_common::WorkerStatusSnapshot;

pub fn format_datetime(value: DateTime<Utc>) -> String {
	value
		.with_timezone(&Local)
		.format("%Y-%m-%d %H:%M:%S")
		.to_string()
}

pub fn external_worker_running(snapshot: Option<&WorkerStatusSnapshot>) -> bool {
	snapshot
		.map(|s| {
			process_exists(s.pid)
				&& !matches!(format!("{:?}", s.phase).as_str(), "Stopped")
		})
		.unwrap_or(false)
}

pub fn process_exists(pid: u32) -> bool {
	match std::env::consts::OS {
		"windows" => false,
		_ => PathBuf::from(format!("/proc/{pid}")).exists(),
	}
}

pub fn open_path(path: &Path) -> Result<(), String> {
	open_url(&path.to_string_lossy())
}

pub fn open_url(target: &str) -> Result<(), String> {
	let status = match std::env::consts::OS {
		"linux" => Command::new("xdg-open").arg(target).status(),
		"macos" => Command::new("open").arg(target).status(),
		"windows" => Command::new("cmd")
			.args(["/C", "start", "", target])
			.status(),
		other => return Err(format!("unsupported platform: {other}")),
	}
	.map_err(|e| e.to_string())?;

	if status.success() {
		Ok(())
	} else {
		Err(format!("command failed with status {status}"))
	}
}
