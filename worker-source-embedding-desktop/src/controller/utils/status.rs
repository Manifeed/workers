use manifeed_worker_common::{
    ReleaseCheckStatus, ServerConnectionState, WorkerPhase, WorkerReleaseStatus,
    WorkerStatusSnapshot,
};

use crate::process::worker_status_is_stale;

use super::sanitized_optional_detail;
use crate::controller::state::{UiNotice, WorkerStatusTone};

pub(crate) fn worker_requires_update(release_status: Option<&WorkerReleaseStatus>) -> bool {
    release_status
        .map(|status| {
            matches!(
                status.status,
                ReleaseCheckStatus::UpdateAvailable | ReleaseCheckStatus::Incompatible
            )
        })
        .unwrap_or(false)
}

pub(crate) fn worker_visual_status(
    snapshot: Option<&WorkerStatusSnapshot>,
    running: bool,
    recent_processing: bool,
) -> (WorkerStatusTone, &'static str) {
    if !running {
        return (WorkerStatusTone::Inactive, "Inactive");
    }

    let Some(snapshot) = snapshot else {
        return (WorkerStatusTone::Inactive, "Inactive");
    };

    if worker_status_is_stale(snapshot) || matches!(snapshot.phase, WorkerPhase::Stopped) {
        return (WorkerStatusTone::Inactive, "Inactive");
    }

    if matches!(snapshot.phase, WorkerPhase::Error)
        || matches!(
            snapshot.server_connection,
            ServerConnectionState::Disconnected
        )
    {
        return (WorkerStatusTone::Error, "Error");
    }

    if matches!(snapshot.phase, WorkerPhase::Processing)
        || snapshot.current_task.is_some()
        || recent_processing
    {
        return (WorkerStatusTone::Processing, "Processing");
    }

    if matches!(snapshot.phase, WorkerPhase::Starting) {
        return (WorkerStatusTone::Active, "Starting");
    }

    (WorkerStatusTone::Active, "Active")
}

pub(crate) fn worker_status_notice(snapshot: Option<&WorkerStatusSnapshot>) -> Option<UiNotice> {
    let snapshot = snapshot?;
    if worker_status_is_stale(snapshot) {
        return None;
    }

    if matches!(snapshot.phase, WorkerPhase::Error) {
        return Some(UiNotice::danger(compact_status_detail(
            snapshot.last_error.as_deref(),
            "Worker error",
        )));
    }

    if matches!(
        snapshot.server_connection,
        ServerConnectionState::Disconnected
    ) {
        return Some(UiNotice::danger(compact_status_detail(
            snapshot.last_error.as_deref(),
            "Backend offline",
        )));
    }

    None
}

pub(crate) fn compact_status_detail(detail: Option<&str>, fallback: &str) -> String {
    let Some(detail) = sanitized_optional_detail(detail) else {
        return fallback.to_string();
    };
    let normalized = detail.to_ascii_lowercase();

    if normalized.contains("invalid_worker_api_key")
        || normalized.contains("invalid api key")
        || normalized.contains("api error (401)")
        || normalized.contains("api error (403)")
        || normalized.contains("not authorized")
        || normalized.contains("forbidden")
    {
        return "Invalid API key".to_string();
    }
    if normalized.contains("builder error")
        || normalized.contains("api error (404)")
        || normalized.contains("not found")
        || normalized.contains("relative url")
        || normalized.contains("invalid url")
    {
        return "Invalid API URL".to_string();
    }
    if normalized.contains("timed out") || normalized.contains("timeout") {
        return "Request timeout".to_string();
    }
    if normalized.contains("error sending request")
        || normalized.contains("connection refused")
        || normalized.contains("dns error")
        || normalized.contains("failed to lookup")
        || normalized.contains("tcp connect")
        || normalized.contains("backend unreachable")
    {
        return "Backend offline".to_string();
    }
    if normalized.contains("api error (5")
        || normalized.contains("bad gateway")
        || normalized.contains("service unavailable")
        || normalized.contains("internal server error")
    {
        return "Backend unavailable".to_string();
    }
    if normalized.contains("response decode")
        || normalized.contains("invalid json")
        || normalized.contains("invalid response")
    {
        return "Invalid response".to_string();
    }

    fallback.to_string()
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
