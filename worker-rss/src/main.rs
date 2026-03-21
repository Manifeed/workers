use std::time::Duration;

use tracing::{error, info};
use worker_rss::api::HttpRssGateway;
use worker_rss::config::RssWorkerConfig;
use worker_rss::feed::HttpFeedFetcher;
use worker_rss::logging::enable_stdout_logs;
use worker_rss::worker::RssWorker;

const RUN_ERROR_SLEEP_SECONDS: u64 = 3;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stdout_log_mode = std::env::args()
        .skip(1)
        .any(|argument| argument == "log" || argument == "--log");

    if stdout_log_mode {
        enable_stdout_logs();
    }
    tracing_subscriber::fmt().with_target(false).init();

    let config = RssWorkerConfig::from_env()?;
    let gateway = HttpRssGateway::new(&config)?;
    let fetcher = HttpFeedFetcher::new(
        config.host_max_requests_per_second,
        config.request_timeout_seconds,
        config.fetch_retry_count,
    )?;
    let mut worker = RssWorker::new(
        gateway,
        fetcher,
        config.max_in_flight_requests,
        config.max_claimed_tasks,
        config.max_in_flight_requests_per_host,
    );

    info!(
        api_url = %config.api_url,
        worker_name = %config.auth.worker_name,
        "worker_rss rust worker started"
    );
    loop {
        match worker.run_once().await {
            Ok(processed) => {
                if !processed {
                    tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
                }
            }
            Err(error) => {
                error!("worker_rss iteration failed: {}", error);
                tokio::time::sleep(Duration::from_secs(RUN_ERROR_SLEEP_SECONDS)).await;
            }
        }
    }
}
