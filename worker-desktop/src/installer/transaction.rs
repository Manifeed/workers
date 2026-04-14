use std::fs;
use std::path::{Path, PathBuf};

use super::bundle::InstalledBundle;

pub(super) struct InstalledBundleTransaction {
    pub(super) installed_bundle: InstalledBundle,
    current_dir: PathBuf,
    backup_dir: PathBuf,
}

impl InstalledBundleTransaction {
    pub(super) fn new(
        installed_bundle: InstalledBundle,
        current_dir: PathBuf,
        backup_dir: PathBuf,
    ) -> Self {
        Self {
            installed_bundle,
            current_dir,
            backup_dir,
        }
    }

    pub(super) fn commit(self) {
        if self.backup_dir.exists() {
            let _ = fs::remove_dir_all(&self.backup_dir);
        }
    }

    pub(super) fn rollback(self) -> Result<(), String> {
        rollback_current_installation(&self.current_dir, &self.backup_dir)
    }
}

pub(super) struct RemovalTransaction {
    current_dir: PathBuf,
    backup_dir: PathBuf,
}

impl RemovalTransaction {
    pub(super) fn stage(current_dir: &Path) -> Result<Self, String> {
        let backup_dir = current_dir
            .parent()
            .ok_or_else(|| "Parent installation directory not found".to_string())?
            .join(".backup-current");

        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir).map_err(|error| error.to_string())?;
        }
        if current_dir.exists() {
            fs::rename(current_dir, &backup_dir).map_err(|error| error.to_string())?;
        }

        Ok(Self {
            current_dir: current_dir.to_path_buf(),
            backup_dir,
        })
    }

    pub(super) fn rollback(&self) -> Result<(), String> {
        rollback_current_installation(&self.current_dir, &self.backup_dir)
    }

    pub(super) fn commit(&self) -> Result<(), String> {
        if self.backup_dir.exists() {
            fs::remove_dir_all(&self.backup_dir).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

pub(super) fn replace_current_installation(
    install_parent: &Path,
    bundle_root: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    let current_dir = install_parent.join("current");
    let backup_dir = install_parent.join(".backup-current");

    if backup_dir.exists() {
        fs::remove_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    }
    if current_dir.exists() {
        fs::rename(&current_dir, &backup_dir).map_err(|error| error.to_string())?;
    }

    if let Err(error) = fs::rename(bundle_root, &current_dir) {
        if backup_dir.exists() {
            let _ = fs::rename(&backup_dir, &current_dir);
        }
        return Err(error.to_string());
    }

    Ok((current_dir, backup_dir))
}

fn rollback_current_installation(current_dir: &Path, backup_dir: &Path) -> Result<(), String> {
    if backup_dir.exists() {
        if current_dir.exists() {
            fs::remove_dir_all(current_dir).map_err(|error| error.to_string())?;
        }
        fs::rename(backup_dir, current_dir).map_err(|error| error.to_string())?;
        return Ok(());
    }

    if current_dir.exists() {
        fs::remove_dir_all(current_dir).map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{rollback_current_installation, RemovalTransaction};

    #[test]
    fn rollback_restores_previous_installation_when_backup_exists() {
        let root = unique_temp_dir();
        let current_dir = root.join("current");
        let backup_dir = root.join(".backup-current");
        fs::create_dir_all(&current_dir).unwrap();
        fs::create_dir_all(&backup_dir).unwrap();
        fs::write(current_dir.join("bundle.txt"), "new").unwrap();
        fs::write(backup_dir.join("bundle.txt"), "old").unwrap();

        rollback_current_installation(&current_dir, &backup_dir).unwrap();

        assert_eq!(
            fs::read_to_string(current_dir.join("bundle.txt")).unwrap(),
            "old"
        );
        assert!(!backup_dir.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rollback_removes_new_installation_when_no_backup_exists() {
        let root = unique_temp_dir();
        let current_dir = root.join("current");
        let backup_dir = root.join(".backup-current");
        fs::create_dir_all(&current_dir).unwrap();
        fs::write(current_dir.join("bundle.txt"), "new").unwrap();

        rollback_current_installation(&current_dir, &backup_dir).unwrap();

        assert!(!current_dir.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staged_uninstall_can_be_rolled_back() {
        let root = unique_temp_dir();
        let current_dir = root.join("current");
        fs::create_dir_all(&current_dir).unwrap();
        fs::write(current_dir.join("bundle.txt"), "current").unwrap();

        let transaction = RemovalTransaction::stage(&current_dir).unwrap();
        assert!(!current_dir.exists());

        transaction.rollback().unwrap();
        assert_eq!(
            fs::read_to_string(current_dir.join("bundle.txt")).unwrap(),
            "current"
        );

        let _ = fs::remove_dir_all(root);
    }

    fn unique_temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "manifeed-install-bundle-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
