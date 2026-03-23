use std::path::Path;
use std::process::Command;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
struct ProbeOutput {
	recommended_backend: String,
	recommended_runtime_bundle: String,
	#[serde(default)]
	notes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct GpuSupport {
	pub recommended_backend: Option<String>,
	pub recommended_runtime_bundle: Option<String>,
	pub notes: Vec<String>,
	pub error: Option<String>,
}

impl GpuSupport {
	pub fn is_supported(&self) -> bool {
		matches!(
			self.recommended_backend.as_deref(),
			Some("cuda") | Some("webgpu")
		)
	}

	pub fn summary(&self) -> String {
		if let Some(error) = &self.error {
			return error.clone();
		}
		if self.is_supported() {
			let backend = self.recommended_backend.as_deref().unwrap_or("gpu");
			let bundle = self.recommended_runtime_bundle.as_deref().unwrap_or("auto");
			return format!("GPU disponible via {backend} ({bundle})");
		}
		self.notes
			.first()
			.cloned()
			.unwrap_or_else(|| "aucun backend GPU detecte".to_string())
	}

	pub fn probe(binary: &Path, config: &Path) -> Self {
		let output = Command::new(binary)
			.arg("probe")
			.arg("--config")
			.arg(config)
			.arg("--acceleration")
			.arg("gpu")
			.output();

		match output {
			Ok(out) if out.status.success() => {
				match serde_json::from_slice::<ProbeOutput>(&out.stdout) {
					Ok(probe) => Self {
						recommended_backend: Some(probe.recommended_backend),
						recommended_runtime_bundle: Some(probe.recommended_runtime_bundle),
						notes: probe.notes,
						error: None,
					},
					Err(e) => Self {
						error: Some(format!("probe GPU invalide: {e}")),
						..Self::default()
					},
				}
			}
			Ok(out) => {
				let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
				Self {
					error: Some(if stderr.is_empty() {
						format!("probe GPU en echec ({})", out.status)
					} else {
						stderr
					}),
					..Self::default()
				}
			}
			Err(e) => Self {
				error: Some(format!("impossible d'executer le probe GPU: {e}")),
				..Self::default()
			},
		}
	}
}
