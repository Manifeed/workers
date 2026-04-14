use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;
use tokenizers::Tokenizer;
use tracing::info;

use crate::error::{EmbeddingWorkerError, Result};
use crate::runtime::{
    available_execution_providers, execution_providers, probe_system, ExecutionBackend,
};

const DEFAULT_MAX_SEQUENCE_LENGTH: usize = 512;

pub struct OnnxEmbedder {
    runtime: Mutex<OnnxRuntime>,
}

struct OnnxRuntime {
    session: Session,
    tokenizer: Tokenizer,
    max_sequence_length: usize,
    has_token_type_ids: bool,
}

#[derive(Debug, Deserialize)]
struct OnnxModelConfig {
    max_position_embeddings: Option<usize>,
}

impl OnnxEmbedder {
    pub fn new(model_dir: impl Into<PathBuf>, execution_backend: ExecutionBackend) -> Result<Self> {
        let model_dir = model_dir.into();
        validate_model_dir(&model_dir)?;

        let config = read_model_config(&model_dir)?;
        let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;
        let model_path = model_dir.join("model.onnx");
        let session = build_session(&model_path, execution_backend)?;
        let has_token_type_ids = session_has_token_type_ids(&session);

        Ok(Self {
            runtime: Mutex::new(OnnxRuntime {
                session,
                tokenizer,
                max_sequence_length: config
                    .max_position_embeddings
                    .unwrap_or(DEFAULT_MAX_SEQUENCE_LENGTH),
                has_token_type_ids,
            }),
        })
    }

    pub async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let mut runtime = self.runtime.lock().map_err(|_| {
            EmbeddingWorkerError::Runtime("embedding runtime mutex poisoned".to_string())
        })?;
        let batch = encode_batch(&runtime.tokenizer, inputs, runtime.max_sequence_length)?;
        let has_token_type_ids = runtime.has_token_type_ids;
        run_batch(runtime.session(), has_token_type_ids, &batch)
    }
}

impl OnnxRuntime {
    fn session(&mut self) -> &mut Session {
        &mut self.session
    }
}

fn build_session(model_path: &Path, execution_backend: ExecutionBackend) -> Result<Session> {
    let probe = probe_system(execution_backend, None);
    let provider_order = execution_providers(execution_backend, &probe);
    let provider_names = provider_order
        .iter()
        .map(|provider| format!("{provider:?}"))
        .collect::<Vec<_>>();
    let available_providers = available_execution_providers()
        .into_iter()
        .map(|provider| provider.to_string())
        .collect::<Vec<_>>();

    let session = Session::builder()
        .map_err(map_ort_error)?
        .with_execution_providers(provider_order)
        .map_err(map_ort_error)?
        .commit_from_file(model_path)
        .map_err(map_ort_error)?;
    info!(
        preferred_execution_backend = %execution_backend,
        recommended_execution_backend = %probe.recommended_backend,
        available_execution_providers = %available_providers.join(","),
        requested_execution_providers = %provider_names.join(","),
        "initialized onnx session"
    );
    Ok(session)
}

fn session_has_token_type_ids(session: &Session) -> bool {
    session
        .inputs()
        .iter()
        .any(|input| input.name() == "token_type_ids")
}

fn run_batch(
    session: &mut Session,
    has_token_type_ids: bool,
    batch: &EncodedBatch,
) -> Result<Vec<Vec<f32>>> {
    let outputs = if has_token_type_ids {
        session
            .run(ort::inputs! {
                "input_ids" => Tensor::from_array((batch.input_shape(), batch.input_ids.clone().into_boxed_slice()))
                    .map_err(map_ort_error)?,
                "attention_mask" => Tensor::from_array((
                    batch.input_shape(),
                    batch.attention_mask.clone().into_boxed_slice(),
                ))
                .map_err(map_ort_error)?,
                "token_type_ids" => Tensor::from_array((
                    batch.input_shape(),
                    batch.token_type_ids.clone().into_boxed_slice(),
                ))
                .map_err(map_ort_error)?,
            })
            .map_err(map_ort_error)?
    } else {
        session
            .run(ort::inputs! {
                "input_ids" => Tensor::from_array((batch.input_shape(), batch.input_ids.clone().into_boxed_slice()))
                    .map_err(map_ort_error)?,
                "attention_mask" => Tensor::from_array((
                    batch.input_shape(),
                    batch.attention_mask.clone().into_boxed_slice(),
                ))
                .map_err(map_ort_error)?,
            })
            .map_err(map_ort_error)?
    };

    let output = &outputs[0];
    let (shape, hidden_states) = output.try_extract_tensor::<f32>().map_err(map_ort_error)?;
    let shape = shape
        .iter()
        .map(|dimension| {
            usize::try_from(*dimension).map_err(|_| {
                EmbeddingWorkerError::Runtime("invalid output tensor dimension".to_string())
            })
        })
        .collect::<Result<Vec<_>>>()?;

    pool_embeddings(hidden_states, shape, &batch.attention_mask)
}

fn read_model_config(model_dir: &Path) -> Result<OnnxModelConfig> {
    let config_path = model_dir.join("config.json");
    let bytes =
        fs::read(config_path).map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;
    serde_json::from_slice::<OnnxModelConfig>(&bytes)
        .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))
}

fn validate_model_dir(model_dir: &Path) -> Result<()> {
    for required_file in ["model.onnx", "tokenizer.json", "config.json"] {
        if !model_dir.join(required_file).exists() {
            return Err(EmbeddingWorkerError::Runtime(format!(
                "missing required model artifact: {}",
                model_dir.join(required_file).display()
            )));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct EncodedBatch {
    batch_size: usize,
    sequence_length: usize,
    input_ids: Vec<i64>,
    attention_mask: Vec<i64>,
    token_type_ids: Vec<i64>,
}

impl EncodedBatch {
    fn input_shape(&self) -> [usize; 2] {
        [self.batch_size, self.sequence_length]
    }
}

fn encode_batch(
    tokenizer: &Tokenizer,
    inputs: &[String],
    max_sequence_length: usize,
) -> Result<EncodedBatch> {
    let encodings = tokenizer
        .encode_batch(inputs.to_vec(), true)
        .map_err(|error| EmbeddingWorkerError::Runtime(error.to_string()))?;

    let sequence_length = encodings
        .iter()
        .map(|encoding| encoding.len().min(max_sequence_length))
        .max()
        .unwrap_or(1)
        .max(1);

    let batch_size = encodings.len();
    let total_items = batch_size * sequence_length;
    let mut input_ids = vec![0_i64; total_items];
    let mut attention_mask = vec![0_i64; total_items];
    let mut token_type_ids = vec![0_i64; total_items];

    for (row_index, encoding) in encodings.iter().enumerate() {
        let ids = encoding.get_ids();
        let mask = encoding.get_attention_mask();
        let type_ids = encoding.get_type_ids();
        let effective_length = ids.len().min(sequence_length);

        for (column_index, id) in ids.iter().enumerate().take(effective_length) {
            let offset = row_index * sequence_length + column_index;
            input_ids[offset] = i64::from(*id);
            attention_mask[offset] = i64::from(mask.get(column_index).copied().unwrap_or(1));
            token_type_ids[offset] = i64::from(type_ids.get(column_index).copied().unwrap_or(0));
        }
    }

    Ok(EncodedBatch {
        batch_size,
        sequence_length,
        input_ids,
        attention_mask,
        token_type_ids,
    })
}

fn pool_embeddings(
    hidden_states: &[f32],
    shape: Vec<usize>,
    attention_mask: &[i64],
) -> Result<Vec<Vec<f32>>> {
    if shape.len() != 3 {
        return Err(EmbeddingWorkerError::Runtime(format!(
            "unexpected embedding tensor rank: expected 3, got {}",
            shape.len()
        )));
    }

    let batch_size = shape[0];
    let sequence_length = shape[1];
    let hidden_size = shape[2];

    if hidden_states.len() != batch_size * sequence_length * hidden_size {
        return Err(EmbeddingWorkerError::Runtime(format!(
            "unexpected embedding tensor size: expected {}, got {}",
            batch_size * sequence_length * hidden_size,
            hidden_states.len()
        )));
    }
    if attention_mask.len() != batch_size * sequence_length {
        return Err(EmbeddingWorkerError::Runtime(format!(
            "unexpected attention mask size: expected {}, got {}",
            batch_size * sequence_length,
            attention_mask.len()
        )));
    }

    let mut pooled = Vec::with_capacity(batch_size);
    for batch_index in 0..batch_size {
        let mut vector = vec![0.0_f32; hidden_size];
        let mut token_count = 0.0_f32;

        for token_index in 0..sequence_length {
            let mask_value = attention_mask[batch_index * sequence_length + token_index];
            if mask_value <= 0 {
                continue;
            }

            token_count += 1.0;
            let hidden_offset = (batch_index * sequence_length + token_index) * hidden_size;
            for dimension in 0..hidden_size {
                vector[dimension] += hidden_states[hidden_offset + dimension];
            }
        }

        if token_count > 0.0 {
            for value in &mut vector {
                *value /= token_count;
            }
        }
        normalize(&mut vector);
        pooled.push(vector);
    }

    Ok(pooled)
}

fn normalize(values: &mut [f32]) {
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in values.iter_mut() {
            *value /= norm;
        }
    }
}

fn map_ort_error<T>(error: ort::Error<T>) -> EmbeddingWorkerError {
    EmbeddingWorkerError::Runtime(error.to_string())
}
