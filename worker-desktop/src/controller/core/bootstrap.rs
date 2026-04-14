use std::path::PathBuf;

use manifeed_worker_common::{load_workers_config, resolve_workers_config_path, WorkersConfig};

use super::super::state::UiNotice;
use super::super::utils::summarize_detail;

#[derive(Clone, Debug)]
pub(crate) enum ConfigAccess {
    Writable,
    ReadOnly { reason: String },
}

pub(super) fn bootstrap_config_state() -> (
    Option<PathBuf>,
    WorkersConfig,
    ConfigAccess,
    Option<UiNotice>,
) {
    match resolve_workers_config_path(None) {
        Ok(path) => match load_workers_config(Some(&path)) {
            Ok((_, config)) => (Some(path), config, ConfigAccess::Writable, None),
            Err(error) => {
                let reason = format!(
                    "Could not load workers config from {}. {} The app is in read-only mode until the file is fixed.",
                    path.display(),
                    summarize_detail(&error.to_string())
                );
                (
                    Some(path),
                    WorkersConfig::default(),
                    ConfigAccess::ReadOnly {
                        reason: reason.clone(),
                    },
                    Some(UiNotice::danger(reason)),
                )
            }
        },
        Err(error) => {
            let reason = format!(
                "Could not resolve the workers config location. {} The app is in read-only mode.",
                summarize_detail(&error.to_string())
            );
            (
                None,
                WorkersConfig::default(),
                ConfigAccess::ReadOnly {
                    reason: reason.clone(),
                },
                Some(UiNotice::danger(reason)),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use manifeed_worker_common::WorkersConfig;

    use super::ConfigAccess;

    #[test]
    fn bootstrap_uses_read_only_mode_when_config_load_fails() {
        let path = PathBuf::from("/tmp/workers.json");
        let (config_path, config, access, notice) =
            bootstrap_config_state_with(Ok(path.clone()), Err("invalid json".to_string()));

        assert_eq!(config_path, Some(path));
        assert_eq!(config.api_url, WorkersConfig::default().api_url);
        assert!(matches!(access, ConfigAccess::ReadOnly { .. }));
        assert_eq!(
            notice.unwrap().to_view().text.to_string(),
            "Could not load workers config from /tmp/workers.json. invalid json The app is in read-only mode until the file is fixed."
        );
    }

    #[test]
    fn bootstrap_uses_read_only_mode_when_path_resolution_fails() {
        let (_, _, access, notice) =
            bootstrap_config_state_with(Err("no home".to_string()), Ok(WorkersConfig::default()));

        assert!(matches!(access, ConfigAccess::ReadOnly { .. }));
        assert_eq!(
            notice.unwrap().to_view().text.to_string(),
            "Could not resolve the workers config location. no home The app is in read-only mode."
        );
    }

    fn bootstrap_config_state_with(
        resolved_path: Result<PathBuf, String>,
        loaded_config: Result<WorkersConfig, String>,
    ) -> (
        Option<PathBuf>,
        WorkersConfig,
        ConfigAccess,
        Option<crate::controller::state::UiNotice>,
    ) {
        match resolved_path {
            Ok(path) => match loaded_config {
                Ok(config) => (Some(path), config, ConfigAccess::Writable, None),
                Err(error) => {
                    let reason = format!(
                        "Could not load workers config from {}. {} The app is in read-only mode until the file is fixed.",
                        path.display(),
                        error
                    );
                    (
                        Some(path),
                        WorkersConfig::default(),
                        ConfigAccess::ReadOnly {
                            reason: reason.clone(),
                        },
                        Some(crate::controller::state::UiNotice::danger(reason)),
                    )
                }
            },
            Err(error) => {
                let reason = format!(
                    "Could not resolve the workers config location. {} The app is in read-only mode.",
                    error
                );
                (
                    None,
                    WorkersConfig::default(),
                    ConfigAccess::ReadOnly {
                        reason: reason.clone(),
                    },
                    Some(crate::controller::state::UiNotice::danger(reason)),
                )
            }
        }
    }
}
