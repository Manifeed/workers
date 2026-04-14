mod config;
mod doctor;
mod probe;
mod run;
mod service;
mod shared;
mod version;

use crate::cli::{Command, RunArgs};

pub(crate) async fn dispatch(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Run(args) => run::run_command(args).await,
        Command::Probe(args) => probe::probe_command(args),
        Command::Config(args) => config::config_command(args),
        Command::Doctor(args) => doctor::doctor_command(args),
        Command::Version(args) => version::version_command(args),
        Command::Service(args) => service::service_command(args),
    }
}

pub(crate) fn default_command() -> Command {
    Command::Run(RunArgs::default())
}
