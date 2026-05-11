use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use crawler_rss::config::{CrawlerRssConfigFile, RssWorkerConfig, RssWorkerConfigOverrides};
use crawler_rss::config_set::{set_or_initialize_config, SetConfigInput};
use crawler_rss::fetch::HttpFeedFetcher;
use crawler_rss::github_update::{update_from_github, GithubUpdateOptions};
use crawler_rss::logging::enable_stdout_logs;
use crawler_rss::paths::app_paths;
use crawler_rss::post_result::HttpRssGateway;
use crawler_rss::release::{check_worker_release_status, ReleaseCheckStatus};
use crawler_rss::runtime::RssWorker;
use crawler_rss::types::WorkerType;
use serde_json::json;
use tracing::{error, info, warn};

const RUN_ERROR_SLEEP_SECONDS: u64 = 3;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_GITHUB_REPOSITORY: &str = "Manifeed/workers";

#[derive(Parser)]
#[command(name = "crawler_rss")]
#[command(about = "Manifeed RSS crawler")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Run(RunArgs),
    Set(SetArgs),
    Update(UpdateArgs),
}

#[derive(Args, Clone, Debug, Default)]
struct RunArgs {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(long)]
    concurrency: Option<usize>,
    #[arg(long)]
    log: bool,
}

#[derive(Args, Clone, Debug, Default)]
struct SetArgs {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(long)]
    concurrency: Option<usize>,
    #[arg(long)]
    show: bool,
}

#[derive(Args, Clone, Debug)]
struct UpdateArgs {
    #[arg(long, env = "MANIFEED_WORKER_GITHUB_REPOSITORY", default_value = DEFAULT_GITHUB_REPOSITORY)]
    repository: String,
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_target(false).init();

    match cli.command.unwrap_or(Command::Run(RunArgs::default())) {
        Command::Update(args) => update_command(args).await,
        Command::Set(args) => {
            let config = set_command(args)?;
            print_config(&config, false)?;
            Ok(())
        }
        Command::Run(args) => {
            let set_args = SetArgs {
                config: args.config.clone(),
                url: args.url.clone(),
                api_key: args.api_key.clone(),
                concurrency: args.concurrency,
                show: false,
            };
            set_command(set_args)?;
            run_command(args).await
        }
    }
}

async fn run_command(args: RunArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if args.log {
        enable_stdout_logs();
    }

    let config = RssWorkerConfig::load(RssWorkerConfigOverrides {
        config_path: args.config,
        api_url: args.url,
        api_key: args.api_key,
        concurrency: args.concurrency,
    })?;
    validate_release_status().await?;

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
        config.max_in_flight_requests_per_host,
    );

    info!(
        api_url = %config.api_url,
        config_path = %config.config_path.display(),
        "crawler_rss started"
    );
    loop {
        match worker.run_once().await {
            Ok(processed) => {
                if !processed {
                    tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
                }
            }
            Err(error) if error.is_auth_error() => {
                error!("crawler_rss fatal authentication error: {}", error);
                return Err(Box::new(error));
            }
            Err(error) => {
                error!("crawler_rss iteration failed: {}", error);
                tokio::time::sleep(Duration::from_secs(RUN_ERROR_SLEEP_SECONDS)).await;
            }
        }
    }
}

fn set_command(
    args: SetArgs,
) -> Result<crawler_rss::config::CrawlerRssConfigFile, Box<dyn std::error::Error + Send + Sync>> {
    let config_value = set_or_initialize_config(SetConfigInput {
        config_path: args.config,
        url: args.url,
        api_key: args.api_key,
        concurrency: args.concurrency,
    })?;
    if args.show {
        print_config(&config_value, false)?;
    }
    Ok(config_value)
}

async fn update_command(args: UpdateArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let update = update_from_github(GithubUpdateOptions {
        repository: args.repository,
        current_version: APP_VERSION.to_string(),
        dry_run: args.dry_run,
    })
    .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::to_value(update)?)?
    );
    Ok(())
}

fn print_config(
    config: &CrawlerRssConfigFile,
    show_secrets: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let api_key = if show_secrets || config.api_key.is_empty() {
        config.api_key.clone()
    } else {
        redact_secret(&config.api_key)
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "api_url": config.api_url,
            "api_key": api_key,
            "concurrency": config.concurrency,
        }))?
    );
    Ok(())
}

fn redact_secret(value: &str) -> String {
    if value.len() <= 8 {
        return "********".to_string();
    }
    format!("{}***{}", &value[..4], &value[value.len() - 4..])
}

async fn validate_release_status() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let github_repository = std::env::var("MANIFEED_WORKER_GITHUB_REPOSITORY")
        .unwrap_or_else(|_| DEFAULT_GITHUB_REPOSITORY.to_string());
    let release = check_worker_release_status(
        &github_repository,
        WorkerType::RssScrapper.cli_product(),
        APP_VERSION,
        &app_paths()?
            .version_cache_dir()
            .join(format!("{}.json", WorkerType::RssScrapper.cli_product())),
    )
    .await?;
    match release.status {
        ReleaseCheckStatus::Incompatible => {
            return Err(release
                .message
                .unwrap_or_else(|| "worker version is no longer supported".to_string())
                .into());
        }
        ReleaseCheckStatus::UpdateAvailable | ReleaseCheckStatus::Unverified => {
            if let Some(message) = release.message {
                warn!("{message}");
            }
        }
        ReleaseCheckStatus::UpToDate => {}
    }
    Ok(())
}
