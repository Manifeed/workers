use std::path::Path;

use chrono::{Duration as ChronoDuration, Utc};
use manifeed_worker_common::{WorkerPhase, WorkerStatusSnapshot, WorkerType};

mod identity;
mod platform;

use platform::process_exists;

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
        "linux" => {
            platform::linux_process_matches_worker(snapshot.pid, expected_binary, worker_type)
        }
        "macos" => {
            platform::macos_process_matches_worker(snapshot.pid, expected_binary, worker_type)
        }
        _ => true,
    }
}

pub use platform::{open_external_url, terminate_process};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::{Duration as ChronoDuration, Utc};
    use manifeed_worker_common::{
        NetworkTotalsSnapshot, ServerConnectionState, WorkerPhase, WorkerStatusSnapshot,
    };

    use super::{identity::process_identity_matches_worker, worker_status_is_stale};

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
