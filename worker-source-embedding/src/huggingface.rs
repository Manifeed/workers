use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::config::EmbeddingWorkerConfig;
use crate::error::{EmbeddingWorkerError, Result};
use crate::onnx::OnnxEmbedder;
use crate::runtime::ExecutionBackend;
use crate::status::WorkerStatusHandle;
use crate::worker::ModelEmbedder;

#[derive(Clone, Copy, Debug)]
struct ArtifactSpec {
    local_name: &'static str,
    remote_candidates: &'static [&'static str],
    optional: bool,
}

const REQUIRED_ARTIFACTS: &[ArtifactSpec] = &[
    ArtifactSpec {
        local_name: "model.onnx",
        remote_candidates: &["onnx/model.onnx", "model.onnx"],
        optional: false,
    },
    ArtifactSpec {
        local_name: "tokenizer.json",
        remote_candidates: &["tokenizer.json", "onnx/tokenizer.json"],
        optional: false,
    },
    ArtifactSpec {
        local_name: "config.json",
        remote_candidates: &["config.json", "onnx/config.json"],
        optional: false,
    },
];

const OPTIONAL_ARTIFACTS: &[ArtifactSpec] = &[
    ArtifactSpec {
        local_name: "model.onnx_data",
        remote_candidates: &["onnx/model.onnx_data", "model.onnx_data"],
        optional: true,
    },
    ArtifactSpec {
        local_name: "tokenizer_config.json",
        remote_candidates: &["tokenizer_config.json", "onnx/tokenizer_config.json"],
        optional: true,
    },
    ArtifactSpec {
        local_name: "special_tokens_map.json",
        remote_candidates: &["special_tokens_map.json", "onnx/special_tokens_map.json"],
        optional: true,
    },
];

const DOWNLOAD_STATUS_UPDATE_INTERVAL: Duration = Duration::from_secs(1);
const DOWNLOAD_STATUS_UPDATE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct HuggingFaceModelReference {
    repo_id: String,
    revision: String,
}

impl HuggingFaceModelReference {
    fn parse(model_name: &str, default_revision: &str) -> Result<Self> {
        let trimmed = model_name.trim();
        if trimmed.is_empty() {
            return Err(EmbeddingWorkerError::InvalidModelReference(
                "model name is empty".to_string(),
            ));
        }

        let (repo_id, revision) = match trimmed.rsplit_once('@') {
            Some((repo_id, revision)) if !repo_id.trim().is_empty() && !revision.trim().is_empty() => {
                (repo_id.trim().to_string(), revision.trim().to_string())
            }
            Some(_) => {
                return Err(EmbeddingWorkerError::InvalidModelReference(format!(
                    "invalid model reference: {trimmed}"
                )))
            }
            None => (trimmed.to_string(), default_revision.to_string()),
        };

        Ok(Self { repo_id, revision })
    }

    fn requested_name(&self, default_revision: &str) -> String {
        if self.revision == default_revision {
            self.repo_id.clone()
        } else {
            format!("{}@{}", self.repo_id, self.revision)
        }
    }

    fn cache_dir(&self, cache_root: &Path) -> PathBuf {
        let mut path = cache_root.to_path_buf();
        for segment in self.repo_id.split('/') {
            if !segment.trim().is_empty() {
                path.push(sanitize_path_component(segment.trim()));
            }
        }
        path.push(sanitize_path_component(&self.revision));
        path
    }
}

struct LoadedModel {
    reference: HuggingFaceModelReference,
    embedder: OnnxEmbedder,
}

pub struct HuggingFaceOnnxModelManager {
    client: Client,
    cache_root: PathBuf,
    base_url: String,
    default_revision: String,
    execution_backend: ExecutionBackend,
    status: WorkerStatusHandle,
    token: Option<String>,
    current_model: Option<LoadedModel>,
}

impl HuggingFaceOnnxModelManager {
    pub fn new(config: &EmbeddingWorkerConfig, status: WorkerStatusHandle) -> Result<Self> {
        let client = Client::builder()
            .user_agent("manifeed-worker-source-embedding/0.1.0")
            .build()
            .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;
        Ok(Self {
            client,
            cache_root: config.model_cache_dir.clone(),
            base_url: config.huggingface_base_url.trim_end_matches('/').to_string(),
            default_revision: config.huggingface_default_revision.clone(),
            execution_backend: config.execution_backend,
            status,
            token: config.huggingface_token.clone(),
            current_model: None,
        })
    }

    async fn ensure_model_loaded(&mut self, model_name: &str) -> Result<()> {
        let reference = HuggingFaceModelReference::parse(model_name, &self.default_revision)?;
        if self
            .current_model
            .as_ref()
            .map(|loaded| loaded.reference == reference)
            .unwrap_or(false)
        {
            return Ok(());
        }

        if let Some(previous_model) = self.current_model.take() {
            info!(
                previous_model = %previous_model.reference.requested_name(&self.default_revision),
                "unloaded embedding model"
            );
        }

        let local_dir = self.ensure_model_cached(&reference).await?;
        let embedder = OnnxEmbedder::new(local_dir.clone(), self.execution_backend)?;
        info!(
            embedding_model_name = %reference.requested_name(&self.default_revision),
            model_dir = %local_dir.display(),
            "loaded embedding model"
        );
        self.current_model = Some(LoadedModel { reference, embedder });
        Ok(())
    }

    async fn ensure_model_cached(&self, reference: &HuggingFaceModelReference) -> Result<PathBuf> {
        let model_dir = reference.cache_dir(&self.cache_root);
        fs::create_dir_all(&model_dir).map_err(|error| {
            EmbeddingWorkerError::Runtime(format!(
                "unable to create model cache directory {}: {error}",
                model_dir.display()
            ))
        })?;

        for artifact in REQUIRED_ARTIFACTS {
            self.ensure_artifact(reference, &model_dir, artifact).await?;
        }
        for artifact in OPTIONAL_ARTIFACTS {
            self.ensure_artifact(reference, &model_dir, artifact).await?;
        }
        Ok(model_dir)
    }

    async fn ensure_artifact(
        &self,
        reference: &HuggingFaceModelReference,
        model_dir: &Path,
        artifact: &ArtifactSpec,
    ) -> Result<()> {
        let local_path = model_dir.join(artifact.local_name);
        if local_path.exists() {
            return Ok(());
        }

        for remote_path in artifact.remote_candidates {
            if self
                .download_artifact(reference, model_dir, artifact.local_name, remote_path)
                .await?
            {
                return Ok(());
            }
        }

        if artifact.optional {
            return Ok(());
        }

        Err(EmbeddingWorkerError::MissingModelArtifact {
            model_name: reference.requested_name(&self.default_revision),
            artifact_name: artifact.local_name.to_string(),
        })
    }

    async fn download_artifact(
        &self,
        reference: &HuggingFaceModelReference,
        model_dir: &Path,
        local_name: &str,
        remote_path: &str,
    ) -> Result<bool> {
        let url = format!(
            "{}/{}/resolve/{}/{}",
            self.base_url, reference.repo_id, reference.revision, remote_path
        );
        let mut request = self.client.get(url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        let mut response = request
            .send()
            .await
            .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if !response.status().is_success() {
            return Err(EmbeddingWorkerError::Runtime(format!(
                "unable to download {remote_path} for {}: http {}",
                reference.requested_name(&self.default_revision),
                response.status()
            )));
        }

        let temporary_path = model_dir.join(format!(".{local_name}.part"));
        let final_path = model_dir.join(local_name);
        let mut file = tokio::fs::File::create(&temporary_path)
            .await
            .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;
        let mut downloaded_bytes = 0_u64;
        let mut pending_status_bytes = 0_u64;
        let mut last_status_update = Instant::now();

        info!(
            embedding_model_name = %reference.requested_name(&self.default_revision),
            artifact = local_name,
            remote_path,
            content_length = response.content_length(),
            cache_path = %final_path.display(),
            "starting onnx artifact download"
        );

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?
        {
            let chunk_len = chunk.len() as u64;
            downloaded_bytes = downloaded_bytes.saturating_add(chunk_len);
            pending_status_bytes = pending_status_bytes.saturating_add(chunk_len);
            file.write_all(&chunk)
                .await
                .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;

            if pending_status_bytes >= DOWNLOAD_STATUS_UPDATE_BYTES
                || last_status_update.elapsed() >= DOWNLOAD_STATUS_UPDATE_INTERVAL
            {
                let _ = self.status.record_transfer(0, pending_status_bytes);
                pending_status_bytes = 0;
                last_status_update = Instant::now();
            }
        }
        file.flush()
            .await
            .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;
        drop(file);

        fs::rename(&temporary_path, &final_path)
            .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;
        if pending_status_bytes > 0 {
            let _ = self.status.record_transfer(0, pending_status_bytes);
        }
        info!(
            embedding_model_name = %reference.requested_name(&self.default_revision),
            artifact = local_name,
            remote_path,
            cache_path = %final_path.display(),
            downloaded_bytes,
            "downloaded onnx artifact"
        );
        Ok(true)
    }
}

#[async_trait]
impl ModelEmbedder for HuggingFaceOnnxModelManager {
    async fn embed(&mut self, model_name: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        self.ensure_model_loaded(model_name).await?;
        let loaded_model = self
            .current_model
            .as_ref()
            .ok_or_else(|| EmbeddingWorkerError::Runtime("no embedding model loaded".to_string()))?;
        loaded_model.embedder.embed(inputs).await
    }
}

fn sanitize_path_component(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
            sanitized.push(character);
        } else {
            sanitized.push('_');
        }
    }
    if sanitized.is_empty() {
        return "main".to_string();
    }
    sanitized
}
