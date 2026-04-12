use chrono::Utc;
use manifeed_worker_common::{
    NetworkTotalsSnapshot, ServerConnectionState, WorkerPhase, WorkerStatusSnapshot,
};

use super::{apply_status_load_result, StatusLoadResult};
use crate::controller::state::WorkerRuntimeState;

#[test]
fn invalid_status_keeps_previous_snapshot_and_sets_warning() {
    let now = Utc::now();
    let previous = sample_snapshot(now, WorkerPhase::Idle);
    let mut state = WorkerRuntimeState {
        status_snapshot: Some(previous.clone()),
        ..WorkerRuntimeState::default()
    };

    apply_status_load_result(
        &mut state,
        StatusLoadResult::Error {
            prefix: "Worker status file is invalid.",
            detail: "invalid json".to_string(),
        },
        std::time::Instant::now(),
    );

    assert_eq!(
        state.status_snapshot.as_ref().map(|snapshot| snapshot.pid),
        Some(previous.pid)
    );
    assert_eq!(
        state
            .status_file_notice
            .as_ref()
            .map(|notice| notice.to_view().text.to_string()),
        Some("Worker status file is invalid. invalid json".to_string())
    );
}

fn sample_snapshot(
    last_updated_at: chrono::DateTime<chrono::Utc>,
    phase: WorkerPhase,
) -> WorkerStatusSnapshot {
    WorkerStatusSnapshot {
        app_version: "0.1.0".to_string(),
        worker_type: "rss_scrapper".to_string(),
        acceleration_mode: None,
        execution_backend: None,
        pid: 1,
        phase,
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
