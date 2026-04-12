use manifeed_worker_common::{WorkerType, WorkersConfig};

use crate::controller::core::{AppCore, ConfigAccess};
use crate::controller::state::WorkerRuntimeState;

#[cfg(unix)]
#[test]
fn running_worker_cannot_be_uninstalled() {
    let mut core = AppCore {
        config_path: None,
        config: WorkersConfig::default(),
        config_access: ConfigAccess::Writable,
        app_release_status: None,
        rss: WorkerRuntimeState::default(),
        embedding: WorkerRuntimeState::default(),
        gpu_support: None,
        global_notice: None,
        app_busy: false,
    };
    core.rss.child = Some(
        std::process::Command::new("sleep")
            .arg("1")
            .spawn()
            .unwrap(),
    );

    let snapshot = core.worker_snapshot(WorkerType::RssScrapper, std::time::Instant::now());

    assert!(!snapshot.can_uninstall);
    assert!(!snapshot.can_install_or_update);

    core.stop_all_children();
}

#[cfg(unix)]
#[test]
fn running_worker_with_available_update_keeps_update_visible_but_disabled() {
    let mut core = AppCore {
        config_path: None,
        config: WorkersConfig::default(),
        config_access: ConfigAccess::Writable,
        app_release_status: None,
        rss: WorkerRuntimeState::default(),
        embedding: WorkerRuntimeState::default(),
        gpu_support: None,
        global_notice: None,
        app_busy: false,
    };
    core.rss.child = Some(
        std::process::Command::new("sleep")
            .arg("1")
            .spawn()
            .unwrap(),
    );
    core.rss.release_status = Some(manifeed_worker_common::WorkerReleaseStatus {
        current_version: "0.1.0".to_string(),
        platform: "linux".to_string(),
        arch: "x86_64".to_string(),
        status: manifeed_worker_common::ReleaseCheckStatus::UpdateAvailable,
        manifest: None,
        checked_at: chrono::Utc::now(),
        from_cache: false,
        message: None,
    });

    let snapshot = core.worker_snapshot(WorkerType::RssScrapper, std::time::Instant::now());

    assert!(snapshot.show_install_action);
    assert!(!snapshot.can_install_or_update);

    core.stop_all_children();
}
