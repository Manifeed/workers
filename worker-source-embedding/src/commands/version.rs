use manifeed_worker_common::{app_paths, check_worker_release_status, WorkerType};
use serde_json::json;

use crate::cli::CommonConfigArgs;

use super::shared::{version_command_default_api_url, APP_VERSION};

pub(crate) fn version_command(_args: CommonConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let version_cache_path = app_paths()?.version_cache_dir().join(format!(
        "{}.json",
        WorkerType::SourceEmbedding.cli_product()
    ));
    let release = check_worker_release_status(
        version_command_default_api_url(),
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
