use manifeed_worker_common::{
    load_workers_config, resolve_workers_config_path, save_workers_config,
};
use serde_json::json;

use crate::cli::{ConfigArgs, ConfigCommand, ConfigField};

use super::shared::{
    parse_acceleration_mode, parse_runtime_bundle, parse_service_mode, redact_secret,
};

pub(crate) fn config_command(args: ConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
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
                    "api_url": config_value.api_url,
                    "embedding": {
                        "enabled": config_value.embedding.enabled,
                        "api_key": api_key,
                        "service_mode": config_value.embedding.service_mode,
                        "installed_version": config_value.embedding.installed_version,
                        "worker_version": config_value.embedding.worker_version,
                        "inference_batch_size": config_value.embedding.inference_batch_size,
                        "acceleration_mode": config_value.embedding.acceleration_mode,
                        "runtime_bundle": config_value.embedding.runtime_bundle,
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
                ConfigField::ApiUrl => config_value.api_url = value,
                ConfigField::ApiKey => config_value.embedding.api_key = value,
                ConfigField::Acceleration => {
                    config_value.embedding.acceleration_mode = parse_acceleration_mode(&value)?;
                }
                ConfigField::RuntimeBundle => {
                    config_value.embedding.runtime_bundle = parse_runtime_bundle(&value)?;
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
