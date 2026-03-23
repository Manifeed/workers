use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use manifeed_worker_common::{
    app_paths, check_worker_connection, check_worker_release_status, install_user_service,
    load_workers_config, resolve_workers_config_path, save_workers_config, start_user_service,
    stop_user_service, uninstall_user_service, ReleaseCheckStatus, ServiceMode, WorkerStatusHandle,
    WorkerStatusInit, WorkerType,
};
use serde_json::json;
use tracing::{error, info, warn};
use worker_rss::api::HttpRssGateway;
use worker_rss::config::{RssWorkerConfig, RssWorkerConfigOverrides};
use worker_rss::feed::HttpFeedFetcher;
use worker_rss::logging::enable_stdout_logs;
use worker_rss::worker::RssWorker;

const RUN_ERROR_SLEEP_SECONDS: u64 = 3;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "worker-rss")]
#[command(about = "Manifeed RSS worker")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Run(RunArgs),
    Install(InstallArgs),
    Config(ConfigArgs),
    Doctor(CommonConfigArgs),
    Version(CommonConfigArgs),
    Service(ServiceArgs),
}

#[derive(Args, Clone, Debug, Default)]
struct CommonConfigArgs {
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Args, Clone, Debug, Default)]
struct RunArgs {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    api_url: Option<String>,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(long)]
    log: bool,
}

#[derive(Args, Clone, Debug)]
struct InstallArgs {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    api_url: String,
    #[arg(long)]
    api_key: String,
    #[arg(long)]
    install_service: bool,
}

#[derive(Args, Clone, Debug)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Subcommand, Clone, Debug)]
enum ConfigCommand {
    Show {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        show_secrets: bool,
    },
    Set {
        #[arg(long)]
        config: Option<PathBuf>,
        field: ConfigField,
        value: String,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum ConfigField {
    ApiUrl,
    ApiKey,
    ServiceMode,
}

#[derive(Args, Clone, Debug)]
struct ServiceArgs {
    #[command(subcommand)]
    command: ServiceCommand,
}

#[derive(Subcommand, Clone, Debug)]
enum ServiceCommand {
    Install {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Start,
    Stop,
    Uninstall,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_target(false).init();

    match cli.command.unwrap_or(Command::Run(RunArgs::default())) {
        Command::Run(args) => run_command(args).await,
        Command::Install(args) => install_command(args),
        Command::Config(args) => config_command(args),
        Command::Doctor(args) => doctor_command(args),
        Command::Version(args) => version_command(args),
        Command::Service(args) => service_command(args),
    }
}

async fn run_command(args: RunArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if args.log {
        enable_stdout_logs();
    }

    let config = RssWorkerConfig::load(RssWorkerConfigOverrides {
        config_path: args.config,
        api_url: args.api_url,
        api_key: args.api_key,
    })?;
    validate_release_status(&config.api_url)?;

    let status = WorkerStatusHandle::new(
        config.status_file_path.clone(),
        WorkerStatusInit {
            worker_type: WorkerType::RssScrapper,
            app_version: APP_VERSION.to_string(),
            acceleration_mode: None,
            execution_backend: None,
        },
    )?;
    let gateway = HttpRssGateway::new(&config, status.clone())?;
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
        config_path = %config.config_path.display(),
        status_file_path = %config.status_file_path.display(),
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
                let _ = status.mark_error(error.to_string());
                error!("worker_rss iteration failed: {}", error);
                tokio::time::sleep(Duration::from_secs(RUN_ERROR_SLEEP_SECONDS)).await;
            }
        }
    }
}

fn install_command(args: InstallArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let current_exe = std::env::current_exe()?;
    let config_path = resolve_workers_config_path(args.config.as_deref())?;
    let (_, mut config) = load_workers_config(Some(config_path.as_path()))?;
    config.install_worker(
        WorkerType::RssScrapper,
        args.api_url.clone(),
        args.api_key.clone(),
        Some(current_exe.clone()),
    );
    if args.install_service {
        config.rss.service_mode = ServiceMode::Background;
    }
    save_workers_config(&config_path, &config)?;

    if args.install_service {
        install_user_service(
            WorkerType::RssScrapper,
            current_exe.as_path(),
            config_path.as_path(),
        )?;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "worker_type": WorkerType::RssScrapper.as_str(),
            "config_path": config_path,
            "binary_path": current_exe,
            "service_mode": config.rss.service_mode,
        }))?
    );
    Ok(())
}

fn config_command(args: ConfigArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match args.command {
        ConfigCommand::Show {
            config,
            show_secrets,
        } => {
            let (config_path, config_value) = load_workers_config(config.as_deref())?;
            let api_key = if show_secrets || config_value.rss.api_key.is_empty() {
                config_value.rss.api_key.clone()
            } else {
                redact_secret(&config_value.rss.api_key)
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "config_path": config_path,
                    "rss": {
                        "enabled": config_value.rss.enabled,
                        "api_url": config_value.rss.api_url,
                        "api_key": api_key,
                        "service_mode": config_value.rss.service_mode,
                        "binary_path": config_value.rss.binary_path,
                        "poll_seconds": config_value.rss.poll_seconds,
                        "lease_seconds": config_value.rss.lease_seconds,
                        "host_max_requests_per_second": config_value.rss.host_max_requests_per_second,
                        "max_in_flight_requests": config_value.rss.max_in_flight_requests,
                        "max_in_flight_requests_per_host": config_value.rss.max_in_flight_requests_per_host,
                        "max_claimed_tasks": config_value.rss.max_claimed_tasks,
                        "request_timeout_seconds": config_value.rss.request_timeout_seconds,
                        "fetch_retry_count": config_value.rss.fetch_retry_count,
                    }
                }))?
            );
            Ok(())
        }
        ConfigCommand::Set {
            config,
            field,
            value,
        } => {
            let config_path = resolve_workers_config_path(config.as_deref())?;
            let (_, mut config_value) = load_workers_config(Some(config_path.as_path()))?;
            match field {
                ConfigField::ApiUrl => config_value.rss.api_url = value,
                ConfigField::ApiKey => config_value.rss.api_key = value,
                ConfigField::ServiceMode => {
                    config_value.rss.service_mode = parse_service_mode(&value)?;
                }
            }
            config_value.rss.enabled = true;
            save_workers_config(&config_path, &config_value)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "config_path": config_path}))?
            );
            Ok(())
        }
    }
}

fn doctor_command(args: CommonConfigArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = RssWorkerConfig::load(RssWorkerConfigOverrides {
        config_path: args.config.clone(),
        api_url: None,
        api_key: None,
    })?;
    let app_dirs = app_paths()?;
    let worker_paths = app_dirs.worker_paths(WorkerType::RssScrapper);
    let (_, stored_config) = load_workers_config(Some(config.config_path.as_path()))?;
    let connection = check_worker_connection(&config.api_url, config.auth.api_key.as_str()).ok();
    let release = check_worker_release_status(
        &config.api_url,
        WorkerType::RssScrapper.cli_product(),
        APP_VERSION,
        config.version_cache_path.as_path(),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "worker_type": WorkerType::RssScrapper.as_str(),
            "app_version": APP_VERSION,
            "config_path": config.config_path,
            "status_file": worker_paths.status_file,
            "log_file": worker_paths.log_file,
            "binary_path": stored_config.rss.binary_path,
            "connection": connection,
            "release": release,
        }))?
    );
    Ok(())
}

fn version_command(args: CommonConfigArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config_path = resolve_workers_config_path(args.config.as_deref())?;
    let (_, config) = load_workers_config(Some(config_path.as_path()))?;
    let version_cache_path = app_paths()?
        .version_cache_dir()
        .join(format!("{}.json", WorkerType::RssScrapper.cli_product()));
    let release = check_worker_release_status(
        config.rss.api_url.as_str(),
        WorkerType::RssScrapper.cli_product(),
        APP_VERSION,
        version_cache_path.as_path(),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "worker_type": WorkerType::RssScrapper.as_str(),
            "version": APP_VERSION,
            "release": release,
        }))?
    );
    Ok(())
}

fn service_command(args: ServiceArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match args.command {
        ServiceCommand::Install { config } => {
            let config_path = resolve_workers_config_path(config.as_deref())?;
            let (_, config_value) = load_workers_config(Some(config_path.as_path()))?;
            let binary_path = config_value
                .rss
                .binary_path
                .unwrap_or(std::env::current_exe()?);
            install_user_service(
                WorkerType::RssScrapper,
                binary_path.as_path(),
                config_path.as_path(),
            )?;
        }
        ServiceCommand::Start => start_user_service(WorkerType::RssScrapper)?,
        ServiceCommand::Stop => stop_user_service(WorkerType::RssScrapper)?,
        ServiceCommand::Uninstall => uninstall_user_service(WorkerType::RssScrapper)?,
    }
    println!("{}", serde_json::to_string_pretty(&json!({"ok": true}))?);
    Ok(())
}

fn parse_service_mode(
    value: &str,
) -> Result<ServiceMode, Box<dyn std::error::Error + Send + Sync>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "manual" => Ok(ServiceMode::Manual),
        "background" => Ok(ServiceMode::Background),
        other => Err(format!("unsupported service mode: {other}").into()),
    }
}

fn redact_secret(value: &str) -> String {
    if value.len() <= 8 {
        return "********".to_string();
    }
    format!("{}***{}", &value[..4], &value[value.len() - 4..])
}

fn validate_release_status(api_url: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let release = check_worker_release_status(
        api_url,
        WorkerType::RssScrapper.cli_product(),
        APP_VERSION,
        &app_paths()?
            .version_cache_dir()
            .join(format!("{}.json", WorkerType::RssScrapper.cli_product())),
    )?;
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
