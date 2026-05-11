use std::path::PathBuf;

use crate::config::{
    load_crawler_rss_config, resolve_crawler_rss_config_path, save_crawler_rss_config,
    CrawlerRssConfigFile,
};
use crate::error::Result;

#[derive(Clone, Debug, Default)]
pub struct SetConfigInput {
    pub config_path: Option<PathBuf>,
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub concurrency: Option<usize>,
}

pub fn set_or_initialize_config(input: SetConfigInput) -> Result<CrawlerRssConfigFile> {
    let config_path = resolve_crawler_rss_config_path(input.config_path.as_deref())?;
    let mut config = load_crawler_rss_config(Some(config_path.as_path()))?;
    if let Some(url) = input.url {
        config.api_url = url;
    }
    if let Some(api_key) = input.api_key {
        config.api_key = api_key;
    }
    if let Some(concurrency) = input.concurrency {
        config.concurrency = concurrency.max(1);
    }
    save_crawler_rss_config(&config_path, &config)?;
    Ok(config)
}
