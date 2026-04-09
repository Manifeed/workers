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

    pub fn section_name(self) -> &'static str {
        match self {
            Self::RssScrapper => "rss",
            Self::SourceEmbedding => "embedding",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::RssScrapper => "RSS",
            Self::SourceEmbedding => "Embedding",
        }
    }

    pub fn binary_name(self) -> &'static str {
        match self {
            Self::RssScrapper => "worker-rss",
            Self::SourceEmbedding => "worker-source-embedding",
        }
    }

    pub fn service_name(self) -> &'static str {
        match self {
            Self::RssScrapper => "manifeed-worker-rss",
            Self::SourceEmbedding => "manifeed-worker-source-embedding",
        }
    }

    pub fn cli_product(self) -> &'static str {
        match self {
            Self::RssScrapper => "rss_worker_bundle",
            Self::SourceEmbedding => "embedding_worker_bundle",
        }
    }

    pub fn desktop_bundle_product(self) -> &'static str {
        let _ = self;
        if std::env::consts::OS == "linux" {
            "manifeed-workers-desktop"
        } else {
            "workers_desktop_app"
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerTaskClaimRequest {
    pub count: u32,
    pub lease_seconds: u32,
    pub worker_version: Option<String>,
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
