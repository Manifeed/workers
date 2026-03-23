use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Result, WorkerError};
use crate::types::WorkerType;

pub fn install_user_service(
    worker_type: WorkerType,
    binary_path: &Path,
    config_path: &Path,
) -> Result<()> {
    match std::env::consts::OS {
        "linux" => install_linux_user_service(worker_type, binary_path, config_path),
        "macos" => install_macos_user_service(worker_type, binary_path, config_path),
        "windows" => install_windows_user_service(worker_type, binary_path, config_path),
        other => Err(WorkerError::Process(format!(
            "unsupported operating system for service installation: {other}"
        ))),
    }
}

pub fn start_user_service(worker_type: WorkerType) -> Result<()> {
    match std::env::consts::OS {
        "linux" => run_command(Command::new("systemctl").args([
            "--user",
            "start",
            worker_type.service_name(),
        ])),
        "macos" => run_command(Command::new("launchctl").args([
            "kickstart",
            "-k",
            &format!("gui/{}/{}", std::process::id(), worker_type.service_name()),
        ])),
        "windows" => {
            run_command(Command::new("schtasks").args(["/Run", "/TN", worker_type.service_name()]))
        }
        other => Err(WorkerError::Process(format!(
            "unsupported operating system for service start: {other}"
        ))),
    }
}

pub fn stop_user_service(worker_type: WorkerType) -> Result<()> {
    match std::env::consts::OS {
        "linux" => run_command(Command::new("systemctl").args([
            "--user",
            "stop",
            worker_type.service_name(),
        ])),
        "macos" => {
            run_command(Command::new("launchctl").args(["remove", worker_type.service_name()]))
        }
        "windows" => {
            run_command(Command::new("schtasks").args(["/End", "/TN", worker_type.service_name()]))
        }
        other => Err(WorkerError::Process(format!(
            "unsupported operating system for service stop: {other}"
        ))),
    }
}

pub fn uninstall_user_service(worker_type: WorkerType) -> Result<()> {
    match std::env::consts::OS {
        "linux" => uninstall_linux_user_service(worker_type),
        "macos" => uninstall_macos_user_service(worker_type),
        "windows" => uninstall_windows_user_service(worker_type),
        other => Err(WorkerError::Process(format!(
            "unsupported operating system for service uninstall: {other}"
        ))),
    }
}

fn install_linux_user_service(
    worker_type: WorkerType,
    binary_path: &Path,
    config_path: &Path,
) -> Result<()> {
    let home = std::env::var("HOME").map_err(|_| {
        WorkerError::Process("HOME is not set for linux user service installation".to_string())
    })?;
    let service_dir = PathBuf::from(home).join(".config/systemd/user");
    fs::create_dir_all(&service_dir)?;
    let service_path = service_dir.join(format!("{}.service", worker_type.service_name()));
    fs::write(
        &service_path,
        format!(
            "[Unit]\nDescription=Manifeed {}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={} run --config {}\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n",
            worker_type.display_name(),
            shell_escape(binary_path),
            shell_escape(config_path),
        ),
    )?;
    run_command(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
    run_command(Command::new("systemctl").args([
        "--user",
        "enable",
        "--now",
        &format!("{}.service", worker_type.service_name()),
    ]))
}

fn uninstall_linux_user_service(worker_type: WorkerType) -> Result<()> {
    let home = std::env::var("HOME").map_err(|_| {
        WorkerError::Process("HOME is not set for linux user service uninstall".to_string())
    })?;
    let service_name = format!("{}.service", worker_type.service_name());
    let service_path = PathBuf::from(home)
        .join(".config/systemd/user")
        .join(&service_name);
    let _ =
        run_command(Command::new("systemctl").args(["--user", "disable", "--now", &service_name]));
    if service_path.exists() {
        fs::remove_file(service_path)?;
    }
    run_command(Command::new("systemctl").args(["--user", "daemon-reload"]))
}

fn install_macos_user_service(
    worker_type: WorkerType,
    binary_path: &Path,
    config_path: &Path,
) -> Result<()> {
    let home = std::env::var("HOME").map_err(|_| {
        WorkerError::Process("HOME is not set for macOS LaunchAgent installation".to_string())
    })?;
    let launch_agents_dir = PathBuf::from(home).join("Library/LaunchAgents");
    fs::create_dir_all(&launch_agents_dir)?;
    let plist_path = launch_agents_dir.join(format!("{}.plist", worker_type.service_name()));
    fs::write(
        &plist_path,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string><string>run</string><string>--config</string><string>{}</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><true/></dict></plist>\n",
            worker_type.service_name(),
            binary_path.display(),
            config_path.display(),
        ),
    )?;
    run_command(Command::new("launchctl").args(["unload", plist_path.to_string_lossy().as_ref()]))
        .ok();
    run_command(Command::new("launchctl").args(["load", plist_path.to_string_lossy().as_ref()]))
}

fn uninstall_macos_user_service(worker_type: WorkerType) -> Result<()> {
    let home = std::env::var("HOME").map_err(|_| {
        WorkerError::Process("HOME is not set for macOS LaunchAgent uninstall".to_string())
    })?;
    let plist_path = PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", worker_type.service_name()));
    if plist_path.exists() {
        let _ = run_command(
            Command::new("launchctl").args(["unload", plist_path.to_string_lossy().as_ref()]),
        );
        fs::remove_file(plist_path)?;
    }
    Ok(())
}

fn install_windows_user_service(
    worker_type: WorkerType,
    binary_path: &Path,
    config_path: &Path,
) -> Result<()> {
    let task_name = worker_type.service_name();
    let command = format!(
        "\"{}\" run --config \"{}\"",
        binary_path.display(),
        config_path.display()
    );
    run_command(Command::new("schtasks").args([
        "/Create", "/F", "/SC", "ONLOGON", "/TN", task_name, "/TR", &command,
    ]))
}

fn uninstall_windows_user_service(worker_type: WorkerType) -> Result<()> {
    run_command(Command::new("schtasks").args(["/Delete", "/F", "/TN", worker_type.service_name()]))
}

fn run_command(command: &mut Command) -> Result<()> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(WorkerError::Process(if stderr.is_empty() {
        format!("command {:?} failed with status {}", command, output.status)
    } else {
        stderr
    }))
}

fn shell_escape(path: &Path) -> String {
    format!("\"{}\"", path.display())
}
