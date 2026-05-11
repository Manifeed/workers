use serde::Deserialize;

use crate::error::{Result, RssWorkerError, WorkerError};
use crate::model::{ClaimedRssTask, RssFeedPayload};
use crate::protocol::WorkerLeaseRead;

#[derive(Deserialize)]
struct RssTaskPayload {
    job_id: String,
    requested_at: chrono::DateTime<chrono::Utc>,
    ingest: bool,
    feeds: Vec<RssFeedPayload>,
}

pub fn parse_claimed_rss_task(lease: WorkerLeaseRead) -> Result<(u64, u64, ClaimedRssTask)> {
    let payload = serde_json::from_value::<RssTaskPayload>(lease.payload)
        .map_err(|error| RssWorkerError::from(WorkerError::ResponseDecode(error.to_string())))?;
    Ok((
        lease.task_id,
        lease.execution_id,
        ClaimedRssTask {
            task_id: lease.task_id,
            execution_id: lease.execution_id,
            job_id: payload.job_id,
            requested_at: payload.requested_at,
            ingest: payload.ingest,
            feeds: payload.feeds,
        },
    ))
}
