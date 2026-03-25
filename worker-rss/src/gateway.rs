use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{Result, RssWorkerError};
use crate::model::RawFeedScrapeResult;

type HmacSha256 = Hmac<Sha256>;

pub const RSS_CONTRACT_VERSION: &str = "rss-worker-result";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerSessionOpenRequest {
    pub task_type: String,
    pub worker_class: String,
    pub worker_version: Option<String>,
    pub client_fingerprint: Option<String>,
    pub session_ttl_seconds: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerSessionOpenRead {
    pub session_id: String,
    pub task_type: String,
    pub worker_class: String,
    pub worker_version: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerTaskClaimRequest {
    pub session_id: String,
    pub task_type: String,
    pub worker_class: String,
    pub worker_version: Option<String>,
    pub queue_lane: String,
    pub count: u32,
    pub lease_seconds: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerLeaseRead {
    pub lease_id: String,
    pub trace_id: String,
    pub task_type: String,
    pub worker_class: String,
    pub worker_version: Option<String>,
    pub queue_lane: String,
    pub task_id: u64,
    pub execution_id: u64,
    pub payload_ref: String,
    pub payload: Value,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub signed_at: DateTime<Utc>,
    pub nonce: String,
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerTaskCompleteRequest {
    pub session_id: String,
    pub lease_id: String,
    pub trace_id: String,
    pub task_type: String,
    pub worker_class: String,
    pub worker_version: Option<String>,
    pub signed_at: String,
    pub nonce: String,
    pub signature: String,
    pub result_payload: WorkerRssTaskResultPayload,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerTaskFailRequest {
    pub session_id: String,
    pub lease_id: String,
    pub trace_id: String,
    pub task_type: String,
    pub worker_class: String,
    pub worker_version: Option<String>,
    pub signed_at: String,
    pub nonce: String,
    pub signature: String,
    pub error_message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerHeartbeatRequest {
    pub session_id: String,
    pub lease_id: Option<String>,
    pub task_type: String,
    pub worker_class: String,
    pub worker_version: Option<String>,
    pub signed_at: String,
    pub nonce: String,
    pub signature: String,
    pub active_task_count: u32,
    pub current_task_label: Option<String>,
    pub last_error: Option<String>,
    pub network_in_bytes: Option<u64>,
    pub network_out_bytes: Option<u64>,
    pub cpu_hint: Option<f64>,
    pub gpu_hint: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerHeartbeatRead {
    pub ok: bool,
    pub session_id: String,
    pub expires_at: DateTime<Utc>,
    pub seen_at: DateTime<Utc>,
}

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

pub fn derive_hmac_secret(api_key: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    hex::encode(digest)
}

pub fn sign_payload(secret: &str, payload: &Value) -> Result<String> {
    let canonical_payload = canonical_json(payload)?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|error| RssWorkerError::Runtime(format!("invalid hmac secret: {error}")))?;
    mac.update(canonical_payload.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

pub fn canonical_json(payload: &Value) -> Result<String> {
    serialize_json_ascii(&normalize_json_value(payload))
}

pub fn utc_timestamp_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn new_nonce() -> String {
    Uuid::new_v4().simple().to_string()
}

fn normalize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(normalize_json_value).collect()),
        Value::Object(object) => {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut normalized = Map::new();
            for key in keys {
                if let Some(item_value) = object.get(&key) {
                    normalized.insert(key, normalize_json_value(item_value));
                }
            }
            Value::Object(normalized)
        }
        _ => value.clone(),
    }
}

fn normalize_local_dedup_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn serialize_json_ascii(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(boolean) => Ok(boolean.to_string()),
        Value::Number(number) => Ok(number.to_string()),
        Value::String(string) => Ok(escape_json_ascii_string(string)),
        Value::Array(items) => {
            let mut serialized_items = Vec::with_capacity(items.len());
            for item in items {
                serialized_items.push(serialize_json_ascii(item)?);
            }
            Ok(format!("[{}]", serialized_items.join(",")))
        }
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let mut serialized_entries = Vec::with_capacity(keys.len());
            for key in keys {
                let item_value = object.get(key).ok_or_else(|| {
                    RssWorkerError::Runtime("missing object entry during canonical json serialization".to_string())
                })?;
                serialized_entries.push(format!(
                    "{}:{}",
                    escape_json_ascii_string(key),
                    serialize_json_ascii(item_value)?
                ));
            }
            Ok(format!("{{{}}}", serialized_entries.join(",")))
        }
    }
}

fn escape_json_ascii_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0C}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control <= '\u{1F}' => {
                escaped.push_str(&format!("\\u{:04x}", control as u32));
            }
            ascii if ascii.is_ascii() => escaped.push(ascii),
            unicode => {
                for unit in unicode.encode_utf16(&mut [0; 2]).iter() {
                    escaped.push_str(&format!("\\u{:04x}", unit));
                }
            }
        }
    }
    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use super::canonical_json;
    use serde_json::json;

    #[test]
    fn canonical_json_escapes_unicode_as_ascii() {
        let payload = json!({
            "b": "économie",
            "a": {
                "title": "L&G conclut un partenariat avec la gestion d’actifs"
            }
        });

        let canonical = canonical_json(&payload).expect("canonical json should serialize");

        assert_eq!(
            canonical,
            r#"{"a":{"title":"L&G conclut un partenariat avec la gestion d\u2019actifs"},"b":"\u00e9conomie"}"#
        );
    }
}
