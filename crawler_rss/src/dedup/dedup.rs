use serde::{Deserialize, Serialize};

use crate::model::RawFeedScrapeResult;

pub const RSS_CONTRACT_VERSION: &str = "rss-worker-result";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerRssTaskResultPayload {
    pub contract_version: String,
    pub result_events: Vec<RawFeedScrapeResult>,
    pub local_dedup: WorkerRssTaskLocalDedup,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerRssTaskLocalDedup {
    pub scope: String,
    pub input_candidates: u32,
    pub output_candidates: u32,
    pub duplicates_dropped: u32,
    pub groups: Vec<WorkerRssLocalDedupGroup>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerRssLocalDedupGroup {
    pub dedup_key: String,
    pub reason: String,
    pub kept_url: Option<String>,
    pub dropped_urls: Vec<String>,
}

pub fn build_rss_task_result_payload(
    results: &[RawFeedScrapeResult],
) -> WorkerRssTaskResultPayload {
    let mut input_candidates = 0u32;
    for result in results {
        for source in &result.sources {
            input_candidates = input_candidates.saturating_add(source.urls.len() as u32);
        }
    }

    WorkerRssTaskResultPayload {
        contract_version: RSS_CONTRACT_VERSION.to_string(),
        result_events: results.to_vec(),
        local_dedup: WorkerRssTaskLocalDedup {
            scope: "task".to_string(),
            input_candidates,
            output_candidates: input_candidates,
            duplicates_dropped: 0,
            groups: Vec::new(),
        },
    }
}
