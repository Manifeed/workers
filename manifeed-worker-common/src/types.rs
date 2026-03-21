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
