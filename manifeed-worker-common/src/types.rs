use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerType {
    #[serde(rename = "rss_scrapper")]
    RssScrapper,
    #[serde(rename = "source_embedding")]
    SourceEmbedding,
}

impl WorkerType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RssScrapper => "rss_scrapper",
            Self::SourceEmbedding => "source_embedding",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerProfile {
    pub identity_id: u64,
    pub worker_type: WorkerType,
    pub device_id: String,
    pub fingerprint: String,
    pub display_name: Option<String>,
    pub hostname: Option<String>,
    pub platform: Option<String>,
    pub arch: Option<String>,
    pub worker_version: Option<String>,
    pub enrollment_status: String,
    pub last_enrolled_at: Option<DateTime<Utc>>,
    pub last_auth_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerAuthChallengeRead {
    pub identity_id: u64,
    pub challenge_id: String,
    pub challenge: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerSessionRead {
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
    pub worker_profile: WorkerProfile,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerMeRead {
    pub worker_profile: WorkerProfile,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerEnrollRequest {
    pub worker_type: WorkerType,
    pub device_id: String,
    pub public_key: String,
    pub hostname: Option<String>,
    pub platform: Option<String>,
    pub arch: Option<String>,
    pub worker_version: Option<String>,
    pub enrollment_token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerAuthChallengeRequest {
    pub worker_type: WorkerType,
    pub device_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerAuthVerifyRequest {
    pub worker_type: WorkerType,
    pub device_id: String,
    pub challenge_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerTaskClaimRequest {
    pub count: u32,
    pub lease_seconds: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerTaskClaim {
    pub task_id: u64,
    pub execution_id: u64,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerTaskCommand {
    pub ok: bool,
}
