use manifeed_worker_common::{
    install_user_service, resolve_workers_config_path, start_user_service, stop_user_service,
    uninstall_user_service, WorkerType,
};
use serde_json::json;

use crate::cli::{ServiceArgs, ServiceCommand};

pub(crate) fn service_command(args: ServiceArgs) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        ServiceCommand::Install { config } => {
            let config_path = resolve_workers_config_path(config.as_deref())?;
            let binary_path = std::env::current_exe()?;
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
