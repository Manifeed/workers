use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use manifeed_worker_common::{
    app_paths, check_worker_connection, check_worker_release_status, install_user_service,
    load_workers_config, resolve_workers_config_path, save_workers_config, start_user_service,
    stop_user_service, uninstall_user_service, AccelerationMode, ReleaseCheckStatus, ServiceMode,
    WorkerStatusHandle, WorkerStatusInit, WorkerType, DEFAULT_API_URL,
};
use serde_json::json;
use tracing::{error, info, warn};
use worker_source_embedding::api::HttpEmbeddingGateway;
use worker_source_embedding::config::{
    EmbeddingWorkerConfig, EmbeddingWorkerConfigOverrides, FIXED_EMBEDDING_MODEL_NAME,
};
use worker_source_embedding::huggingface::HuggingFaceOnnxModelManager;
use worker_source_embedding::runtime::{
    probe_system, verify_execution_backend_support, ExecutionBackend,
};
use worker_source_embedding::worker::EmbeddingWorker;

const RUN_ERROR_SLEEP_SECONDS: u64 = 3;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "worker-source-embedding")]
#[command(about = "Manifeed embedding worker")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Run(RunArgs),
    Probe(ProbeArgs),
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
    api_key: Option<String>,
    #[arg(long, value_enum)]
    acceleration: Option<AccelerationArg>,
    #[arg(long, value_enum)]
    provider: Option<ProviderArg>,
}

#[derive(Args, Clone, Debug)]
struct ProbeArgs {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long, value_enum)]
    acceleration: Option<AccelerationArg>,
    #[arg(long, value_enum)]
    provider: Option<ProviderArg>,
}

#[derive(Args, Clone, Debug)]
struct InstallArgs {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    api_key: String,
    #[arg(long, value_enum)]
    acceleration: Option<AccelerationArg>,
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
    ApiKey,
    Acceleration,
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

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AccelerationArg {
    Auto,
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProviderArg {
    Auto,
    Cpu,
    Cuda,
    Webgpu,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_target(false).init();

    match cli.command.unwrap_or(Command::Run(RunArgs::default())) {
        Command::Run(args) => run_command(args).await,
        Command::Probe(args) => probe_command(args),
        Command::Install(args) => install_command(args),
        Command::Config(args) => config_command(args),
        Command::Doctor(args) => doctor_command(args),
        Command::Version(args) => version_command(args),
        Command::Service(args) => service_command(args),
    }
}

async fn run_command(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config = EmbeddingWorkerConfig::load(EmbeddingWorkerConfigOverrides {
        config_path: args.config,
        api_key: args.api_key,
        acceleration_mode: args.acceleration.map(map_acceleration_arg),
        provider_override: args.provider.map(map_provider_arg),
    })?;
    validate_release_status(&config.api_url)?;

    let ort_runtime_path = verify_execution_backend_support(
        config.execution_backend,
        config.ort_dylib_path.as_deref(),
    )?;
    let probe = probe_system(config.execution_backend, config.ort_dylib_path.as_deref());
    let status = WorkerStatusHandle::new(
        config.status_file_path.clone(),
        WorkerStatusInit {
            worker_type: WorkerType::SourceEmbedding,
            app_version: APP_VERSION.to_string(),
            acceleration_mode: Some(acceleration_mode_label(config.acceleration_mode).to_string()),
            execution_backend: Some(config.execution_backend.to_string()),
        },
    )?;

    let gateway = HttpEmbeddingGateway::new(&config, status.clone())?;
    let embedder = HuggingFaceOnnxModelManager::new(&config, status.clone())?;
    let mut worker = EmbeddingWorker::new(
        gateway,
        embedder,
        config.inference_batch_size,
        status.clone(),
    );

    info!(
        api_url = %config.api_url,
        config_path = %config.config_path.display(),
        worker_version = %config.worker_version,
        embedding_model_name = FIXED_EMBEDDING_MODEL_NAME,
        acceleration_mode = %acceleration_mode_label(config.acceleration_mode),
        execution_backend = %config.execution_backend,
        recommended_execution_backend = %probe.recommended_backend,
        recommended_runtime_bundle = %probe.recommended_runtime_bundle,
        ort_runtime_path = %ort_runtime_path.display(),
        status_file_path = %config.status_file_path.display(),
        model_cache_dir = %config.model_cache_dir.display(),
        "worker_source_embedding rust started"
    );
    if !probe.notes.is_empty() {
        warn!(notes = %probe.notes.join(" | "), "runtime probe warnings");
    }

    loop {
        match worker.run_once().await {
            Ok(processed) => {
                if !processed {
                    tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
                }
            }
            Err(error) if error.is_network_error() => {
                warn!(
                    retry_delay_seconds = config.poll_seconds,
                    "network error in embedding worker loop, retrying: {error}"
                );
                tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
            }
            Err(error) => {
                let _ = status.mark_error(error.to_string());
                error!("worker_source_embedding iteration failed: {}", error);
                tokio::time::sleep(Duration::from_secs(RUN_ERROR_SLEEP_SECONDS)).await;
            }
        }
    }
}

fn probe_command(args: ProbeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let acceleration_mode =
        resolve_probe_acceleration_mode(args.config.as_deref(), args.acceleration)?;
    let provider = args
        .provider
        .map(map_provider_arg)
        .unwrap_or_else(|| match acceleration_mode {
            AccelerationMode::Auto => ExecutionBackend::Auto,
            AccelerationMode::Cpu => ExecutionBackend::Cpu,
            AccelerationMode::Gpu => ExecutionBackend::Auto,
        });
    let ort_dylib_path = app_paths()?
        .worker_paths(WorkerType::SourceEmbedding)
        .install_dir
        .join("runtime/lib/libonnxruntime.so");
    let probe = probe_system(
        provider,
        ort_dylib_path.exists().then_some(ort_dylib_path.as_path()),
    );
    println!("{}", serde_json::to_string_pretty(&probe)?);
    Ok(())
}

fn install_command(args: InstallArgs) -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let config_path = resolve_workers_config_path(args.config.as_deref())?;
    let (_, mut config) = load_workers_config(Some(config_path.as_path()))?;
    config.install_worker(
        WorkerType::SourceEmbedding,
        args.api_key.clone(),
        Some(current_exe.clone()),
    );
    if let Some(acceleration) = args.acceleration {
        config.embedding.acceleration_mode = map_acceleration_arg(acceleration);
    }
    if args.install_service {
        config.embedding.service_mode = ServiceMode::Background;
    }
    save_workers_config(&config_path, &config)?;

    if args.install_service {
        install_user_service(
            WorkerType::SourceEmbedding,
            current_exe.as_path(),
            config_path.as_path(),
        )?;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "worker_type": WorkerType::SourceEmbedding.as_str(),
            "config_path": config_path,
            "binary_path": current_exe,
            "service_mode": config.embedding.service_mode,
            "acceleration_mode": config.embedding.acceleration_mode,
        }))?
    );
    Ok(())
}

fn config_command(args: ConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        ConfigCommand::Show {
            config,
            show_secrets,
        } => {
            let (config_path, config_value) = load_workers_config(config.as_deref())?;
            let api_key = if show_secrets || config_value.embedding.api_key.is_empty() {
                config_value.embedding.api_key.clone()
            } else {
                redact_secret(&config_value.embedding.api_key)
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "config_path": config_path,
                    "embedding": {
                        "enabled": config_value.embedding.enabled,
                        "api_key": api_key,
                        "service_mode": config_value.embedding.service_mode,
                        "binary_path": config_value.embedding.binary_path,
                        "worker_version": config_value.embedding.worker_version,
                        "inference_batch_size": config_value.embedding.inference_batch_size,
                        "acceleration_mode": config_value.embedding.acceleration_mode,
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
                ConfigField::ApiKey => config_value.embedding.api_key = value,
                ConfigField::Acceleration => {
                    config_value.embedding.acceleration_mode = parse_acceleration_mode(&value)?;
                }
                ConfigField::ServiceMode => {
                    config_value.embedding.service_mode = parse_service_mode(&value)?;
                }
            }
            config_value.embedding.enabled = true;
            save_workers_config(&config_path, &config_value)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "config_path": config_path}))?
            );
            Ok(())
        }
    }
}

fn doctor_command(args: CommonConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config = EmbeddingWorkerConfig::load(EmbeddingWorkerConfigOverrides {
        config_path: args.config.clone(),
        api_key: None,
        acceleration_mode: None,
        provider_override: None,
    })?;
    let app_dirs = app_paths()?;
    let worker_paths = app_dirs.worker_paths(WorkerType::SourceEmbedding);
    let (_, stored_config) = load_workers_config(Some(config.config_path.as_path()))?;
    let connection = check_worker_connection(&config.api_url, config.auth.api_key.as_str()).ok();
    let release = check_worker_release_status(
        &config.api_url,
        WorkerType::SourceEmbedding.cli_product(),
        APP_VERSION,
        config.version_cache_path.as_path(),
    )?;
    let probe = probe_system(config.execution_backend, config.ort_dylib_path.as_deref());
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "worker_type": WorkerType::SourceEmbedding.as_str(),
            "app_version": APP_VERSION,
            "config_path": config.config_path,
            "status_file": worker_paths.status_file,
            "log_file": worker_paths.log_file,
            "model_cache_dir": worker_paths.cache_dir,
            "binary_path": stored_config.embedding.binary_path,
            "acceleration_mode": config.acceleration_mode,
            "execution_backend": config.execution_backend,
            "connection": connection,
            "release": release,
            "probe": probe,
        }))?
    );
    Ok(())
}

fn version_command(_args: CommonConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let version_cache_path = app_paths()?.version_cache_dir().join(format!(
        "{}.json",
        WorkerType::SourceEmbedding.cli_product()
    ));
    let release = check_worker_release_status(
        DEFAULT_API_URL,
        WorkerType::SourceEmbedding.cli_product(),
        APP_VERSION,
        version_cache_path.as_path(),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "worker_type": WorkerType::SourceEmbedding.as_str(),
            "version": APP_VERSION,
            "release": release,
        }))?
    );
    Ok(())
}

fn service_command(args: ServiceArgs) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        ServiceCommand::Install { config } => {
            let config_path = resolve_workers_config_path(config.as_deref())?;
            let (_, config_value) = load_workers_config(Some(config_path.as_path()))?;
            let binary_path = config_value
                .embedding
                .binary_path
                .unwrap_or(std::env::current_exe()?);
            install_user_service(
                WorkerType::SourceEmbedding,
                binary_path.as_path(),
                config_path.as_path(),
            )?;
        }
        ServiceCommand::Start => start_user_service(WorkerType::SourceEmbedding)?,
        ServiceCommand::Stop => stop_user_service(WorkerType::SourceEmbedding)?,
        ServiceCommand::Uninstall => uninstall_user_service(WorkerType::SourceEmbedding)?,
    }
    println!("{}", serde_json::to_string_pretty(&json!({"ok": true}))?);
    Ok(())
}

fn validate_release_status(api_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let release = check_worker_release_status(
        api_url,
        WorkerType::SourceEmbedding.cli_product(),
        APP_VERSION,
        &app_paths()?.version_cache_dir().join(format!(
            "{}.json",
            WorkerType::SourceEmbedding.cli_product()
        )),
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

fn resolve_probe_acceleration_mode(
    config_path: Option<&std::path::Path>,
    acceleration: Option<AccelerationArg>,
) -> Result<AccelerationMode, Box<dyn std::error::Error>> {
    if let Some(acceleration) = acceleration {
        return Ok(map_acceleration_arg(acceleration));
    }
    let (_, config) = load_workers_config(config_path)?;
    Ok(config.embedding.acceleration_mode)
}

fn parse_service_mode(value: &str) -> Result<ServiceMode, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "manual" => Ok(ServiceMode::Manual),
        "background" => Ok(ServiceMode::Background),
        other => Err(format!("unsupported service mode: {other}").into()),
    }
}

fn parse_acceleration_mode(value: &str) -> Result<AccelerationMode, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(AccelerationMode::Auto),
        "cpu" => Ok(AccelerationMode::Cpu),
        "gpu" => Ok(AccelerationMode::Gpu),
        other => Err(format!("unsupported acceleration mode: {other}").into()),
    }
}

fn map_acceleration_arg(value: AccelerationArg) -> AccelerationMode {
    match value {
        AccelerationArg::Auto => AccelerationMode::Auto,
        AccelerationArg::Cpu => AccelerationMode::Cpu,
        AccelerationArg::Gpu => AccelerationMode::Gpu,
    }
}

fn map_provider_arg(value: ProviderArg) -> ExecutionBackend {
    match value {
        ProviderArg::Auto => ExecutionBackend::Auto,
        ProviderArg::Cpu => ExecutionBackend::Cpu,
        ProviderArg::Cuda => ExecutionBackend::Cuda,
        ProviderArg::Webgpu => ExecutionBackend::WebGpu,
    }
}

fn acceleration_mode_label(mode: AccelerationMode) -> &'static str {
    match mode {
        AccelerationMode::Auto => "auto",
        AccelerationMode::Cpu => "cpu",
        AccelerationMode::Gpu => "gpu",
    }
}

fn redact_secret(value: &str) -> String {
    if value.len() <= 8 {
        return "********".to_string();
    }
    format!("{}***{}", &value[..4], &value[value.len() - 4..])
}
