use std::path::Path;
use std::process::Command;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
struct ProbeOutput {
    recommended_backend: String,
    recommended_runtime_bundle: String,
    #[serde(default)]
    available_execution_providers: Vec<String>,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    runtime_load_error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct GpuSupport {
    pub recommended_backend: Option<String>,
    pub recommended_runtime_bundle: Option<String>,
    pub available_execution_providers: Vec<String>,
    pub notes: Vec<String>,
    pub error: Option<String>,
    pub runtime_load_error: Option<String>,
}

impl GpuSupport {
    pub fn is_supported(&self) -> bool {
        matches!(
            self.recommended_backend.as_deref(),
            Some("cuda") | Some("webgpu")
        ) && self
            .recommended_backend
            .as_ref()
            .map(|backend| {
                self.available_execution_providers
                    .iter()
                    .any(|provider| provider == backend)
            })
            .unwrap_or(false)
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
        if let Some(runtime_load_error) = &self.runtime_load_error {
            return format!("runtime ONNX indisponible: {runtime_load_error}");
        }
        if matches!(
            self.recommended_backend.as_deref(),
            Some("cuda") | Some("webgpu")
        ) {
            let backend = self.recommended_backend.as_deref().unwrap_or("gpu");
            let providers = if self.available_execution_providers.is_empty() {
                "aucun provider".to_string()
            } else {
                self.available_execution_providers.join(", ")
            };
            return format!(
				"GPU detectee, mais le runtime ONNX installe n'active pas {backend} (providers: {providers})"
			);
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
                        available_execution_providers: probe.available_execution_providers,
                        notes: probe.notes,
                        runtime_load_error: probe.runtime_load_error,
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

#[cfg(test)]
mod tests {
    use super::GpuSupport;

    #[test]
    fn gpu_support_requires_matching_runtime_provider() {
        let support = GpuSupport {
            recommended_backend: Some("cuda".to_string()),
            available_execution_providers: vec!["cpu".to_string()],
            ..GpuSupport::default()
        };

        assert!(!support.is_supported());
        assert!(support.summary().contains("n'active pas cuda"));
    }

    #[test]
    fn gpu_support_is_available_when_runtime_matches() {
        let support = GpuSupport {
            recommended_backend: Some("cuda".to_string()),
            recommended_runtime_bundle: Some("cuda12".to_string()),
            available_execution_providers: vec!["cuda".to_string(), "cpu".to_string()],
            ..GpuSupport::default()
        };

        assert!(support.is_supported());
        assert_eq!(support.summary(), "GPU disponible via cuda (cuda12)");
    }
}
