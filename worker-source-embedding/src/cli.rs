use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "worker-source-embedding")]
#[command(about = "Manifeed embedding worker")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    Run(RunArgs),
    Probe(ProbeArgs),
    Config(ConfigArgs),
    Doctor(CommonConfigArgs),
    Version(CommonConfigArgs),
    Service(ServiceArgs),
}

#[derive(Args, Clone, Debug, Default)]
pub(crate) struct CommonConfigArgs {
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args, Clone, Debug, Default)]
pub(crate) struct RunArgs {
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
    #[arg(long)]
    pub(crate) api_key: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) acceleration: Option<AccelerationArg>,
    #[arg(long, value_enum)]
    pub(crate) provider: Option<ProviderArg>,
}

#[derive(Args, Clone, Debug)]
pub(crate) struct ProbeArgs {
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub(crate) acceleration: Option<AccelerationArg>,
    #[arg(long, value_enum)]
    pub(crate) provider: Option<ProviderArg>,
}

#[derive(Args, Clone, Debug)]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub(crate) command: ConfigCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum ConfigCommand {
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
pub(crate) enum ConfigField {
    ApiUrl,
    ApiKey,
    Acceleration,
    RuntimeBundle,
    ServiceMode,
}

#[derive(Args, Clone, Debug)]
pub(crate) struct ServiceArgs {
    #[command(subcommand)]
    pub(crate) command: ServiceCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum ServiceCommand {
    Install {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Start,
    Stop,
    Uninstall,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum AccelerationArg {
    Auto,
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum ProviderArg {
    Auto,
    Cpu,
    Cuda,
    Webgpu,
    Coreml,
}
