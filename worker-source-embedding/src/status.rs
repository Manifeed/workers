use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use manifeed_worker_common::api::ApiTrafficObserver;
use serde::{Deserialize, Serialize};

use crate::error::{EmbeddingWorkerError, Result};
use crate::runtime::ExecutionBackend;
use crate::worker::ClaimedEmbeddingTask;

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
    pub job_id: String,
    pub model_name: String,
    pub source_count: usize,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NetworkTotalsSnapshot {
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerStatusSnapshot {
    pub worker_type: String,
    pub execution_backend: String,
    pub pid: u32,
    pub phase: WorkerPhase,
    pub server_connection: ServerConnectionState,
    pub started_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
    pub last_server_contact_at: Option<DateTime<Utc>>,
    pub current_task: Option<CurrentTaskSnapshot>,
    pub completed_task_count: u64,
    pub network_totals: NetworkTotalsSnapshot,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct WorkerStatusHandle {
    inner: Arc<WorkerStatusInner>,
}

struct WorkerStatusInner {
    path: PathBuf,
    snapshot: Mutex<WorkerStatusSnapshot>,
}

impl WorkerStatusHandle {
    pub fn new(path: impl Into<PathBuf>, execution_backend: ExecutionBackend) -> Result<Self> {
        let now = Utc::now();
        let pid = std::process::id();
        let path = path.into();
        let snapshot = WorkerStatusSnapshot {
            worker_type: "source_embedding".to_string(),
            execution_backend: execution_backend.to_string(),
            pid,
            phase: WorkerPhase::Starting,
            server_connection: ServerConnectionState::Unknown,
            started_at: now,
            last_updated_at: now,
            last_server_contact_at: None,
            current_task: None,
            completed_task_count: 0,
            network_totals: NetworkTotalsSnapshot::default(),
            last_error: None,
        };
        let handle = Self {
            inner: Arc::new(WorkerStatusInner {
                path,
                snapshot: Mutex::new(snapshot),
            }),
        };
        handle.persist()?;
        Ok(handle)
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn mark_idle(&self) -> Result<()> {
        self.update(|snapshot| {
            snapshot.phase = WorkerPhase::Idle;
            snapshot.current_task = None;
            snapshot.last_error = None;
        })
    }

    pub fn mark_processing(&self, task: &ClaimedEmbeddingTask) -> Result<()> {
        self.update(|snapshot| {
            snapshot.phase = WorkerPhase::Processing;
            snapshot.current_task = Some(CurrentTaskSnapshot {
                task_id: task.task_id,
                execution_id: task.execution_id,
                job_id: task.job_id.clone(),
                model_name: task.embedding_model_name.clone(),
                source_count: task.sources.len(),
                started_at: Utc::now(),
            });
            snapshot.last_error = None;
        })
    }

    pub fn mark_completed_task(&self) -> Result<()> {
        self.update(|snapshot| {
            snapshot.phase = WorkerPhase::Idle;
            snapshot.current_task = None;
            snapshot.completed_task_count += 1;
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
        self.update(|snapshot| {
            snapshot.server_connection = ServerConnectionState::Disconnected;
            snapshot.last_error = Some(message.clone());
        })
    }

    pub fn mark_error(&self, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        self.update(|snapshot| {
            snapshot.phase = WorkerPhase::Error;
            snapshot.last_error = Some(message.clone());
        })
    }

    pub fn mark_stopped(&self) -> Result<()> {
        self.update(|snapshot| {
            snapshot.phase = WorkerPhase::Stopped;
            snapshot.current_task = None;
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

    fn update(&self, update_fn: impl FnOnce(&mut WorkerStatusSnapshot)) -> Result<()> {
        {
            let mut snapshot = self.inner.snapshot.lock().map_err(|_| {
                EmbeddingWorkerError::Runtime("worker status mutex poisoned".to_string())
            })?;
            update_fn(&mut snapshot);
            snapshot.last_updated_at = Utc::now();
        }
        self.persist()
    }

    fn persist(&self) -> Result<()> {
        let snapshot = self.inner.snapshot.lock().map_err(|_| {
            EmbeddingWorkerError::Runtime("worker status mutex poisoned".to_string())
        })?;
        persist_snapshot(&self.inner.path, &snapshot)
    }
}

impl ApiTrafficObserver for WorkerStatusHandle {
    fn record_transfer(&self, request_bytes: usize, response_bytes: usize) {
        let _ = self.record_transfer(request_bytes as u64, response_bytes as u64);
    }
}

fn persist_snapshot(path: &Path, snapshot: &WorkerStatusSnapshot) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            EmbeddingWorkerError::Runtime(format!(
                "unable to create status directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    let temp_path = path.with_extension("tmp");
    let payload = serde_json::to_vec_pretty(snapshot)
        .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;
    fs::write(&temp_path, payload).map_err(|error| {
        EmbeddingWorkerError::Runtime(format!(
            "unable to write worker status file {}: {error}",
            temp_path.display()
        ))
    })?;
    fs::rename(&temp_path, path).map_err(|error| {
        EmbeddingWorkerError::Runtime(format!(
            "unable to move worker status file into place {}: {error}",
            path.display()
        ))
    })?;
    Ok(())
}
