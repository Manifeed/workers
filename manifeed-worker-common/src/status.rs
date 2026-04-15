use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};

use crate::api::ApiTrafficObserver;
use crate::error::{Result, WorkerError};
use crate::types::WorkerType;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerPhase {
    Starting,
    Idle,
    Processing,
    Error,
    Stopped,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerConnectionState {
    Unknown,
    Connected,
    Disconnected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurrentTaskSnapshot {
    pub task_id: u64,
    pub execution_id: u64,
    pub job_id: Option<String>,
    pub label: Option<String>,
    pub worker_version: Option<String>,
    pub item_count: Option<usize>,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NetworkTotalsSnapshot {
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerStatusSnapshot {
    pub app_version: String,
    pub worker_type: String,
    pub acceleration_mode: Option<String>,
    pub execution_backend: Option<String>,
    pub pid: u32,
    pub phase: WorkerPhase,
    pub server_connection: ServerConnectionState,
    pub started_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
    pub last_server_contact_at: Option<DateTime<Utc>>,
    pub current_task: Option<CurrentTaskSnapshot>,
    pub current_feed_id: Option<u64>,
    pub current_feed_url: Option<String>,
    pub completed_task_count: u64,
    pub network_totals: NetworkTotalsSnapshot,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct WorkerStatusInit {
    pub worker_type: WorkerType,
    pub app_version: String,
    pub acceleration_mode: Option<String>,
    pub execution_backend: Option<String>,
}

#[derive(Clone)]
pub struct WorkerStatusHandle {
    inner: Arc<WorkerStatusInner>,
}

struct WorkerStatusInner {
    path: PathBuf,
    snapshot: Mutex<WorkerStatusSnapshot>,
    last_persisted_at: Mutex<DateTime<Utc>>,
}

const STATUS_PERSIST_INTERVAL_MS: i64 = 1_000;

impl WorkerStatusHandle {
    pub fn new(path: impl Into<PathBuf>, init: WorkerStatusInit) -> Result<Self> {
        let now = Utc::now();
        let path = path.into();
        let handle = Self {
            inner: Arc::new(WorkerStatusInner {
                path,
                snapshot: Mutex::new(WorkerStatusSnapshot {
                    app_version: init.app_version,
                    worker_type: init.worker_type.as_str().to_string(),
                    acceleration_mode: init.acceleration_mode,
                    execution_backend: init.execution_backend,
                    pid: std::process::id(),
                    phase: WorkerPhase::Starting,
                    server_connection: ServerConnectionState::Unknown,
                    started_at: now,
                    last_updated_at: now,
                    last_server_contact_at: None,
                    current_task: None,
                    current_feed_id: None,
                    current_feed_url: None,
                    completed_task_count: 0,
                    network_totals: NetworkTotalsSnapshot::default(),
                    last_error: None,
                }),
                last_persisted_at: Mutex::new(now),
            }),
        };
        handle.persist()?;
        Ok(handle)
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn snapshot(&self) -> Result<WorkerStatusSnapshot> {
        self.inner
            .snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| WorkerError::Process("worker status mutex poisoned".to_string()))
    }

    pub fn mark_idle(&self) -> Result<()> {
        self.update_and_persist(true, |snapshot| {
            snapshot.phase = WorkerPhase::Idle;
            snapshot.current_task = None;
            snapshot.current_feed_id = None;
            snapshot.current_feed_url = None;
            snapshot.last_error = None;
        })
    }

    pub fn mark_processing(&self, task: CurrentTaskSnapshot) -> Result<()> {
        self.update_and_persist(true, |snapshot| {
            snapshot.phase = WorkerPhase::Processing;
            snapshot.current_task = Some(task);
            snapshot.last_error = None;
        })
    }

    pub fn mark_completed_task(&self) -> Result<()> {
        self.update_and_persist(true, |snapshot| {
            snapshot.phase = WorkerPhase::Idle;
            snapshot.current_task = None;
            snapshot.current_feed_id = None;
            snapshot.current_feed_url = None;
            snapshot.completed_task_count = snapshot.completed_task_count.saturating_add(1);
            snapshot.last_error = None;
        })
    }

    pub fn record_completed_task(&self) -> Result<()> {
        self.update(|snapshot| {
            snapshot.completed_task_count = snapshot.completed_task_count.saturating_add(1);
            snapshot.last_error = None;
        })
    }

    pub fn mark_server_connected(&self) -> Result<()> {
        self.update(|snapshot| {
            snapshot.server_connection = ServerConnectionState::Connected;
            snapshot.last_server_contact_at = Some(Utc::now());
        })
    }

    pub fn mark_server_disconnected(&self, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        self.update_and_persist(true, |snapshot| {
            snapshot.server_connection = ServerConnectionState::Disconnected;
            snapshot.last_error = Some(message.clone());
        })
    }

    pub fn mark_error(&self, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        self.update_and_persist(true, |snapshot| {
            snapshot.phase = WorkerPhase::Error;
            snapshot.last_error = Some(message.clone());
        })
    }

    pub fn mark_stopped(&self) -> Result<()> {
        self.update_and_persist(true, |snapshot| {
            snapshot.phase = WorkerPhase::Stopped;
            snapshot.current_task = None;
            snapshot.current_feed_id = None;
            snapshot.current_feed_url = None;
        })
    }

    pub fn set_current_feed(&self, feed_id: Option<u64>, feed_url: Option<String>) -> Result<()> {
        self.update(|snapshot| {
            snapshot.current_feed_id = feed_id;
            snapshot.current_feed_url = feed_url;
        })
    }

    pub fn record_transfer(&self, request_bytes: u64, response_bytes: u64) -> Result<()> {
        self.update(|snapshot| {
            snapshot.network_totals.bytes_sent = snapshot
                .network_totals
                .bytes_sent
                .saturating_add(request_bytes);
            snapshot.network_totals.bytes_received = snapshot
                .network_totals
                .bytes_received
                .saturating_add(response_bytes);
        })
    }

    pub fn update(&self, update_fn: impl FnOnce(&mut WorkerStatusSnapshot)) -> Result<()> {
        self.update_and_persist(false, update_fn)
    }

    pub fn flush(&self) -> Result<()> {
        self.persist_if_due(true)
    }

    fn update_and_persist(
        &self,
        force_persist: bool,
        update_fn: impl FnOnce(&mut WorkerStatusSnapshot),
    ) -> Result<()> {
        {
            let mut snapshot =
                self.inner.snapshot.lock().map_err(|_| {
                    WorkerError::Process("worker status mutex poisoned".to_string())
                })?;
            update_fn(&mut snapshot);
            snapshot.last_updated_at = Utc::now();
        }
        self.persist_if_due(force_persist)
    }

    fn persist(&self) -> Result<()> {
        let snapshot = self
            .inner
            .snapshot
            .lock()
            .map_err(|_| WorkerError::Process("worker status mutex poisoned".to_string()))?
            .clone();
        persist_snapshot(&self.inner.path, &snapshot)?;
        *self
            .inner
            .last_persisted_at
            .lock()
            .map_err(|_| WorkerError::Process("worker status mutex poisoned".to_string()))? =
            snapshot.last_updated_at;
        Ok(())
    }

    fn persist_if_due(&self, force: bool) -> Result<()> {
        let now = Utc::now();
        let last_persisted_at = *self
            .inner
            .last_persisted_at
            .lock()
            .map_err(|_| WorkerError::Process("worker status mutex poisoned".to_string()))?;
        let is_due =
            now - last_persisted_at >= ChronoDuration::milliseconds(STATUS_PERSIST_INTERVAL_MS);
        if force || is_due {
            self.persist()?;
        }
        Ok(())
    }
}

impl ApiTrafficObserver for WorkerStatusHandle {
    fn record_transfer(&self, request_bytes: usize, response_bytes: usize) {
        let _ = self.record_transfer(request_bytes as u64, response_bytes as u64);
    }
}

fn persist_snapshot(path: &Path, snapshot: &WorkerStatusSnapshot) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    let payload = serde_json::to_vec_pretty(snapshot)?;
    fs::write(&temp_path, payload)?;
    fs::rename(&temp_path, path).map_err(|error| {
        WorkerError::Io(std::io::Error::new(
            error.kind(),
            format!(
                "unable to move worker status file into place {}: {error}",
                path.display()
            ),
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::Utc;

    use super::{CurrentTaskSnapshot, WorkerPhase, WorkerStatusHandle, WorkerStatusInit};
    use crate::types::WorkerType;

    #[test]
    fn record_completed_task_preserves_current_phase() {
        let path = std::env::temp_dir().join(format!(
            "manifeed-worker-status-test-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let handle = WorkerStatusHandle::new(
            &path,
            WorkerStatusInit {
                worker_type: WorkerType::RssScrapper,
                app_version: "0.1.0".to_string(),
                acceleration_mode: None,
                execution_backend: None,
            },
        )
        .unwrap();

        handle
            .mark_processing(CurrentTaskSnapshot {
                task_id: 1,
                execution_id: 2,
                job_id: Some("job-1".to_string()),
                label: Some("job".to_string()),
                worker_version: None,
                item_count: Some(1),
                started_at: Utc::now(),
            })
            .unwrap();
        handle.record_completed_task().unwrap();

        let snapshot = handle.snapshot().unwrap();
        assert!(matches!(snapshot.phase, WorkerPhase::Processing));
        assert_eq!(snapshot.completed_task_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn record_transfer_is_coalesced_until_flush() {
        let path = std::env::temp_dir().join(format!(
            "manifeed-worker-status-transfer-test-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let handle = WorkerStatusHandle::new(
            &path,
            WorkerStatusInit {
                worker_type: WorkerType::SourceEmbedding,
                app_version: "0.1.0".to_string(),
                acceleration_mode: None,
                execution_backend: None,
            },
        )
        .unwrap();
        let initial_payload = fs::read_to_string(&path).unwrap();

        handle.record_transfer(10, 20).unwrap();
        let coalesced_payload = fs::read_to_string(&path).unwrap();
        assert_eq!(initial_payload, coalesced_payload);

        handle.flush().unwrap();
        let flushed_payload = fs::read_to_string(&path).unwrap();
        assert_ne!(initial_payload, flushed_payload);

        let _ = fs::remove_file(path);
    }
}
