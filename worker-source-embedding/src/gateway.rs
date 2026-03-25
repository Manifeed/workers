use chrono::{DateTime, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{EmbeddingWorkerError, Result};
use crate::worker::EmbeddingResultSource;

type HmacSha256 = Hmac<Sha256>;

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
    pub result_payload: WorkerEmbeddingTaskResultPayload,
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
pub struct WorkerEmbeddingTaskResultPayload {
    pub sources: Vec<EmbeddingResultSource>,
}

pub fn build_embedding_task_result_payload(
    sources: Vec<EmbeddingResultSource>,
) -> WorkerEmbeddingTaskResultPayload {
    WorkerEmbeddingTaskResultPayload { sources }
}

pub fn derive_hmac_secret(api_key: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    hex::encode(digest)
}

pub fn sign_payload(secret: &str, payload: &Value) -> Result<String> {
    let canonical_payload = canonical_json(payload)?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|error| EmbeddingWorkerError::Runtime(format!("invalid hmac secret: {error}")))?;
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

fn serialize_json_ascii(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(boolean) => Ok(boolean.to_string()),
        Value::Number(number) => Ok(normalize_json_number(number.to_string())),
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
                    EmbeddingWorkerError::Runtime(
                        "missing object entry during canonical json serialization".to_string(),
                    )
                })?;
                serialized_entries.push(format!(
                    "{}:{}",
                    escape_json_ascii_string(key),
                    serialize_json_ascii(item_value)?,
                ));
            }
            Ok(format!("{{{}}}", serialized_entries.join(",")))
        }
    }
}

fn normalize_json_number(value: String) -> String {
    let Some(exponent_index) = value.find('e') else {
        return value;
    };
    let mantissa = &value[..exponent_index];
    let exponent = &value[exponent_index + 1..];
    if exponent.is_empty() {
        return value;
    }
    let (sign, digits) = match exponent.as_bytes()[0] {
        b'+' | b'-' => (&exponent[..1], &exponent[1..]),
        _ => ("+", exponent),
    };
    if digits.is_empty() {
        return value;
    }
    let padded_digits = if digits.len() >= 2 {
        digits.to_string()
    } else {
        format!("0{digits}")
    };
    format!("{mantissa}e{sign}{padded_digits}")
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
                let mut units = [0u16; 2];
                for unit in unicode.encode_utf16(&mut units).iter() {
                    escaped.push_str(&format!("\\u{:04x}", unit));
                }
            }
        }
    }
    escaped.push('"');
    escaped
}
