use std::collections::BTreeMap;

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
    let mut deduped_results = results.to_vec();
    let mut dedup_groups: BTreeMap<String, WorkerRssLocalDedupGroup> = BTreeMap::new();
    let mut input_candidates = 0u32;
    let mut output_candidates = 0u32;

    for result in &mut deduped_results {
        let mut deduped_sources = Vec::new();
        let original_sources = std::mem::take(&mut result.sources);
        for source in original_sources {
            input_candidates = input_candidates.saturating_add(1);
            let dedup_key = normalize_local_dedup_key(&source.url);
            if let Some(group) = dedup_groups.get_mut(&dedup_key) {
                group.dropped_urls.push(source.url.clone());
                continue;
            }
            dedup_groups.insert(
                dedup_key.clone(),
                WorkerRssLocalDedupGroup {
                    dedup_key,
                    reason: "same_url".to_string(),
                    kept_url: Some(source.url.clone()),
                    dropped_urls: Vec::new(),
                },
            );
            output_candidates = output_candidates.saturating_add(1);
            deduped_sources.push(source);
        }
        result.sources = deduped_sources;
    }

    let mut groups = Vec::new();
    for (_, group) in dedup_groups {
        if !group.dropped_urls.is_empty() {
            groups.push(group);
        }
    }

    WorkerRssTaskResultPayload {
        contract_version: RSS_CONTRACT_VERSION.to_string(),
        result_events: deduped_results,
        local_dedup: WorkerRssTaskLocalDedup {
            scope: "task".to_string(),
            input_candidates,
            output_candidates,
            duplicates_dropped: input_candidates.saturating_sub(output_candidates),
            groups,
        },
    }
}

fn normalize_local_dedup_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
