use chrono::{Duration as ChronoDuration, Utc};
use manifeed_worker_common::{
    NetworkTotalsSnapshot, ServerConnectionState, WorkerPhase, WorkerStatusSnapshot,
};

use super::{compact_status_detail, worker_status_notice, worker_visual_status};
use crate::controller::state::WorkerStatusTone;

#[test]
fn worker_visual_status_maps_processing_and_error_states() {
    let now = Utc::now();
    let processing = sample_snapshot(
        now,
        WorkerPhase::Processing,
        ServerConnectionState::Connected,
    );
    let disconnected = sample_snapshot(now, WorkerPhase::Idle, ServerConnectionState::Disconnected);

    assert_eq!(
        worker_visual_status(Some(&processing), true, false),
        (WorkerStatusTone::Processing, "Processing")
    );
    assert_eq!(
        worker_visual_status(Some(&disconnected), true, false),
        (WorkerStatusTone::Error, "Error")
    );
}

#[test]
fn worker_visual_status_marks_stale_snapshots_inactive() {
    let stale = sample_snapshot(
        Utc::now() - ChronoDuration::seconds(180),
        WorkerPhase::Idle,
        ServerConnectionState::Connected,
    );

    assert_eq!(
        worker_visual_status(Some(&stale), true, false),
        (WorkerStatusTone::Inactive, "Inactive")
    );
}

#[test]
fn worker_visual_status_keeps_recent_work_processing() {
    let idle = sample_snapshot(
        Utc::now(),
        WorkerPhase::Idle,
        ServerConnectionState::Connected,
    );

    assert_eq!(
        worker_visual_status(Some(&idle), true, true),
        (WorkerStatusTone::Processing, "Processing")
    );
}

#[test]
fn worker_status_notice_surfaces_runtime_errors() {
    let mut snapshot = sample_snapshot(
        Utc::now(),
        WorkerPhase::Error,
        ServerConnectionState::Connected,
    );
    snapshot.last_error = Some("api error (403): forbidden".to_string());

    let notice = worker_status_notice(Some(&snapshot)).unwrap();
    let view = notice.to_view();

    assert!(view.visible);
    assert_eq!(view.text.to_string(), "Invalid API key");
}

#[test]
fn compact_status_detail_maps_verbose_errors_to_short_labels() {
    assert_eq!(
        compact_status_detail(Some("http error: builder error"), "Worker error"),
        "Invalid API URL"
    );
    assert_eq!(
        compact_status_detail(
            Some("http error: error sending request for url http://example.com"),
            "Worker error",
        ),
        "Backend offline"
    );
}

fn sample_snapshot(
    last_updated_at: chrono::DateTime<chrono::Utc>,
    phase: WorkerPhase,
    server_connection: ServerConnectionState,
) -> WorkerStatusSnapshot {
    WorkerStatusSnapshot {
        app_version: "0.1.0".to_string(),
        worker_type: "rss_scrapper".to_string(),
        acceleration_mode: None,
        execution_backend: None,
        pid: 1,
        phase,
        server_connection,
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
