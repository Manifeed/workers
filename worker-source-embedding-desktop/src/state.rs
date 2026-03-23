use std::process::Child;

use manifeed_worker_common::{
	WorkerConnectionCheck, WorkerReleaseStatus, WorkerStatusSnapshot,
};

#[derive(Default)]
pub struct WorkerUiState {
	pub child: Option<Child>,
	pub status_snapshot: Option<WorkerStatusSnapshot>,
	pub connection_check: Option<WorkerConnectionCheck>,
	pub release_status: Option<WorkerReleaseStatus>,
	pub last_message: Option<String>,
}

pub struct SnapshotSummary {
	pub phase: String,
	pub connection: String,
	pub completed_tasks: String,
}

pub fn summarize(snapshot: Option<&WorkerStatusSnapshot>) -> SnapshotSummary {
	SnapshotSummary {
		phase: snapshot
			.map(|s| format!("{:?}", s.phase).to_lowercase())
			.unwrap_or_else(|| "inconnu".to_string()),
		connection: snapshot
			.map(|s| format!("{:?}", s.server_connection).to_lowercase())
			.unwrap_or_else(|| "inconnu".to_string()),
		completed_tasks: snapshot
			.map(|s| s.completed_task_count.to_string())
			.unwrap_or_else(|| "0".to_string()),
	}
}
