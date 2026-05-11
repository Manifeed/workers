use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerType {
    #[serde(rename = "rss_scrapper")]
    RssScrapper,
}

impl WorkerType {
    pub fn cli_product(self) -> &'static str {
        match self {
            Self::RssScrapper => "crawler_rss_bundle",
        }
    }
}
