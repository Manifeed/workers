use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Result, WorkerError};

pub fn install_archive(archive_path: &Path) -> Result<PathBuf> {
    let work_dir = archive_path.with_extension("extract");
    if work_dir.exists() {
        fs::remove_dir_all(&work_dir)?;
    }
    fs::create_dir_all(&work_dir)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(&work_dir)
        .status()?;
    if !status.success() {
        return Err(WorkerError::Process("unable to extract release archive".to_string()).into());
    }
    let binary = find_extracted_binary(&work_dir)?;
    let target_path = std::env::current_exe()?;
    let backup_path = target_path.with_extension("old");
    let next_path = target_path.with_extension("new");
    fs::copy(&binary, &next_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&next_path, fs::Permissions::from_mode(0o755))?;
    }
    if backup_path.exists() {
        fs::remove_file(&backup_path)?;
    }
    fs::rename(&target_path, &backup_path)?;
    fs::rename(&next_path, &target_path)?;
    Ok(target_path)
}

fn find_extracted_binary(work_dir: &Path) -> Result<PathBuf> {
    let expected_name = if cfg!(windows) {
        "crawler_rss.exe"
    } else {
        "crawler_rss"
    };
    let mut stack = vec![work_dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
                continue;
            }
            if entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == expected_name)
            {
                return Ok(entry_path);
            }
        }
    }
    Err(WorkerError::Version("crawler_rss binary not found in archive".to_string()).into())
}
