use manifeed_worker_common::{EmbeddingRuntimeBundle, ServiceMode, WorkerType, WorkersConfig};

pub const ALL_WORKERS: [WorkerType; 2] = [WorkerType::RssScrapper, WorkerType::SourceEmbedding];

pub fn api_credentials(config: &WorkersConfig, worker_type: WorkerType) -> (&str, &str) {
    (
        config.worker_api_url(worker_type),
        config.worker_api_key(worker_type),
    )
}

pub fn installed_version(config: &WorkersConfig, worker_type: WorkerType) -> &str {
    let version = match worker_type {
        WorkerType::RssScrapper => config.rss.installed_version.as_str(),
        WorkerType::SourceEmbedding => config.embedding.installed_version.as_str(),
    };

    if version.trim().is_empty() {
        "0.0.0"
    } else {
        version
    }
}

pub fn service_mode(config: &WorkersConfig, worker_type: WorkerType) -> ServiceMode {
    match worker_type {
        WorkerType::RssScrapper => config.rss.service_mode,
        WorkerType::SourceEmbedding => config.embedding.service_mode,
    }
}

pub fn release_cache_name(worker_type: WorkerType, runtime_bundle: Option<&str>) -> String {
    match runtime_bundle {
        Some(bundle) => format!("{}-{bundle}.json", worker_type.cli_product()),
        None => release_cache_name_for_product(worker_type.cli_product()),
    }
}

pub fn release_cache_name_for_product(product: &str) -> String {
    format!("{product}.json")
}

pub fn mark_worker_installed(
    config: &mut WorkersConfig,
    worker_type: WorkerType,
    version: String,
    worker_version: Option<String>,
    runtime_bundle: Option<EmbeddingRuntimeBundle>,
) {
    match worker_type {
        WorkerType::RssScrapper => {
            config.rss.enabled = true;
            config.rss.installed_version = version;
        }
        WorkerType::SourceEmbedding => {
            config.embedding.enabled = true;
            config.embedding.installed_version = version;
            if let Some(worker_version) = worker_version {
                config.embedding.worker_version = worker_version;
            }
            if let Some(runtime_bundle) = runtime_bundle {
                config.embedding.runtime_bundle = runtime_bundle;
            }
        }
    }
}

pub fn mark_worker_removed(config: &mut WorkersConfig, worker_type: WorkerType) {
    match worker_type {
        WorkerType::RssScrapper => {
            config.rss.enabled = false;
            config.rss.installed_version.clear();
        }
        WorkerType::SourceEmbedding => {
            config.embedding.enabled = false;
            config.embedding.installed_version.clear();
        }
    }
}
