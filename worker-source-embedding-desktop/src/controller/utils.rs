use chrono::{Duration as ChronoDuration, Utc};
use manifeed_worker_common::{
    user_facing_error_message, AccelerationMode, EmbeddingRuntimeBundle, ReleaseCheckStatus,
    ServerConnectionState, ServiceMode, WorkerError, WorkerPhase, WorkerReleaseStatus,
    WorkerStatusSnapshot, WorkersConfig, DEFAULT_API_URL,
};

use crate::gpu::GpuSupport;
use crate::installer::resolved_runtime_bundle;

use super::state::{UiNotice, WorkerStatusTone};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ServiceSyncAction {
    InstallBackgroundService,
    RemoveBackgroundService,
}

const STATUS_STALE_AFTER_SECONDS: i64 = 120;

pub(super) fn connection_failure_notice(status_code: Option<u16>, detail: Option<&str>) -> String {
    match status_code {
        Some(401 | 403) => "Invalid API key".to_string(),
        Some(404) => "Invalid API URL".to_string(),
        Some(408 | 504) => "Request timeout".to_string(),
        Some(500..=599) => "Backend unavailable".to_string(),
        _ => compact_status_detail(detail, "Request failed"),
    }
}

pub(super) fn connection_error_notice(error: &WorkerError) -> String {
    user_facing_error_message(error)
}

pub(super) fn summarize_detail(detail: &str) -> String {
    sanitized_optional_detail(Some(detail)).unwrap_or_else(|| "Please try again.".to_string())
}

fn sanitized_optional_detail(detail: Option<&str>) -> Option<String> {
    let detail = detail?;
    let first_line = detail.lines().next().unwrap_or_default().trim();
    if first_line.is_empty() {
        return None;
    }

    let collapsed = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }

    let mut summary = collapsed;
    if summary.len() > 140 {
        summary.truncate(137);
        summary.push_str("...");
    }
    Some(summary)
}

pub(super) fn normalize_api_url(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_API_URL.to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

pub(super) fn service_mode_index(mode: ServiceMode) -> i32 {
    match mode {
        ServiceMode::Manual => 0,
        ServiceMode::Background => 1,
    }
}

pub(super) fn service_mode_from_index(index: i32) -> ServiceMode {
    match index {
        1 => ServiceMode::Background,
        _ => ServiceMode::Manual,
    }
}

pub(super) fn acceleration_mode_index(mode: AccelerationMode) -> i32 {
    match mode {
        AccelerationMode::Auto => 0,
        AccelerationMode::Cpu => 1,
        AccelerationMode::Gpu => 2,
    }
}

pub(super) fn acceleration_mode_from_index(index: i32) -> AccelerationMode {
    match index {
        1 => AccelerationMode::Cpu,
        2 => AccelerationMode::Gpu,
        _ => AccelerationMode::Auto,
    }
}

pub(super) fn planned_service_sync(
    previous: ServiceMode,
    next: ServiceMode,
    installed: bool,
) -> Option<ServiceSyncAction> {
    if !installed || previous == next {
        return None;
    }

    match next {
        ServiceMode::Manual => Some(ServiceSyncAction::RemoveBackgroundService),
        ServiceMode::Background => Some(ServiceSyncAction::InstallBackgroundService),
    }
}

pub(super) fn predicted_gpu_support(config: &WorkersConfig) -> GpuSupport {
    if cfg!(target_os = "macos") {
        return GpuSupport {
            recommended_backend: Some("coreml".to_string()),
            recommended_runtime_bundle: Some("coreml".to_string()),
            available_execution_providers: vec!["coreml".to_string()],
            notes: vec!["CoreML will be used after the embedding bundle is installed.".to_string()],
            error: None,
            runtime_load_error: None,
        };
    }

    match resolved_runtime_bundle(config) {
        Ok(EmbeddingRuntimeBundle::Cuda12) => GpuSupport {
            recommended_backend: Some("cuda".to_string()),
            recommended_runtime_bundle: Some("cuda12".to_string()),
            available_execution_providers: vec!["cuda".to_string()],
            notes: vec!["NVIDIA support detected. The CUDA12 bundle will be selected.".to_string()],
            error: None,
            runtime_load_error: None,
        },
        Ok(EmbeddingRuntimeBundle::CoreMl) => GpuSupport {
            recommended_backend: Some("coreml".to_string()),
            recommended_runtime_bundle: Some("coreml".to_string()),
            available_execution_providers: vec!["coreml".to_string()],
            notes: vec!["CoreML will be used after the embedding bundle is installed.".to_string()],
            error: None,
            runtime_load_error: None,
        },
        Ok(EmbeddingRuntimeBundle::None) | Ok(EmbeddingRuntimeBundle::WebGpu) => GpuSupport {
            notes: vec![
                "No compatible GPU bundle was detected. The CPU bundle will be used.".to_string(),
            ],
            ..GpuSupport::default()
        },
        Err(error) => GpuSupport {
            error: Some(error),
            ..GpuSupport::default()
        },
    }
}

pub(super) fn worker_requires_update(release_status: Option<&WorkerReleaseStatus>) -> bool {
    release_status
        .map(|status| {
            matches!(
                status.status,
                ReleaseCheckStatus::UpdateAvailable | ReleaseCheckStatus::Incompatible
            )
        })
        .unwrap_or(false)
}

pub(super) fn worker_is_busy(snapshot: Option<&WorkerStatusSnapshot>) -> bool {
    snapshot
        .map(|snapshot| {
            snapshot.current_task.is_some() || matches!(snapshot.phase, WorkerPhase::Processing)
        })
        .unwrap_or(false)
}

pub(super) fn worker_visual_status(
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

pub(super) fn worker_status_notice(snapshot: Option<&WorkerStatusSnapshot>) -> Option<UiNotice> {
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

fn compact_status_detail(detail: Option<&str>, fallback: &str) -> String {
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

fn worker_status_is_stale(snapshot: &WorkerStatusSnapshot) -> bool {
    snapshot.last_updated_at < Utc::now() - ChronoDuration::seconds(STATUS_STALE_AFTER_SECONDS)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration as ChronoDuration, Utc};
    use manifeed_worker_common::{
        NetworkTotalsSnapshot, ServerConnectionState, ServiceMode, WorkerPhase,
        WorkerStatusSnapshot,
    };

    use super::{
        compact_status_detail, connection_failure_notice, planned_service_sync,
        worker_status_notice, worker_visual_status, ServiceSyncAction,
    };
    use crate::controller::state::WorkerStatusTone;

    #[test]
    fn service_sync_is_only_required_for_installed_workers() {
        assert_eq!(
            planned_service_sync(ServiceMode::Manual, ServiceMode::Background, false),
            None
        );
        assert_eq!(
            planned_service_sync(ServiceMode::Manual, ServiceMode::Background, true),
            Some(ServiceSyncAction::InstallBackgroundService)
        );
        assert_eq!(
            planned_service_sync(ServiceMode::Background, ServiceMode::Manual, true),
            Some(ServiceSyncAction::RemoveBackgroundService)
        );
    }

    #[test]
    fn connection_failures_map_common_http_statuses() {
        assert_eq!(
            connection_failure_notice(Some(403), None),
            "Invalid API key"
        );
        assert_eq!(
            connection_failure_notice(Some(404), None),
            "Invalid API URL"
        );
    }

    #[test]
    fn worker_visual_status_maps_processing_and_error_states() {
        let now = Utc::now();
        let processing = sample_snapshot(
            now,
            WorkerPhase::Processing,
            ServerConnectionState::Connected,
        );
        let disconnected =
            sample_snapshot(now, WorkerPhase::Idle, ServerConnectionState::Disconnected);

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
}
