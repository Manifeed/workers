use std::fs::{self, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::Utc;
use manifeed_worker_common::{
    app_paths, check_worker_connection, check_worker_release_status, install_user_service,
    load_workers_config, save_workers_config, start_user_service, stop_user_service,
    uninstall_user_service, AccelerationMode, ReleaseCheckStatus, ServiceMode,
    WorkerConnectionCheck, WorkerReleaseStatus, WorkerStatusSnapshot, WorkerType,
};

use crate::gpu::GpuSupport;
use crate::helpers::external_worker_running;
use crate::state::WorkerUiState;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppPage {
    Scraping,
    Embedding,
}

pub struct ControlApp {
    pub config_path: PathBuf,
    pub config: manifeed_worker_common::WorkersConfig,
    pub current_page: AppPage,
    pub rss: WorkerUiState,
    pub embedding: WorkerUiState,
    pub gpu_support: Option<GpuSupport>,
    pub last_refresh: Instant,
    pub last_error: Option<String>,
}

impl ControlApp {
    pub fn bootstrap() -> Self {
        let (config_path, config) = load_workers_config(None).unwrap_or_else(|_| {
            (
                PathBuf::from("workers.json"),
                manifeed_worker_common::WorkersConfig::default(),
            )
        });

        let mut app = Self {
            config_path,
            config,
            current_page: AppPage::Scraping,
            rss: WorkerUiState::default(),
            embedding: WorkerUiState::default(),
            gpu_support: None,
            last_refresh: Instant::now() - Duration::from_secs(10),
            last_error: None,
        };
        app.refresh();
        app.refresh_release_statuses();
        app.refresh_gpu_support();
        app
    }

    pub fn refresh(&mut self) {
        self.poll_child(WorkerType::RssScrapper);
        self.poll_child(WorkerType::SourceEmbedding);
        self.refresh_status(WorkerType::RssScrapper);
        self.refresh_status(WorkerType::SourceEmbedding);
        self.last_refresh = Instant::now();
    }

    pub fn refresh_release_statuses(&mut self) {
        self.rss.release_status = self.compute_release_status(WorkerType::RssScrapper);
        self.embedding.release_status = self.compute_release_status(WorkerType::SourceEmbedding);
    }

    pub(crate) fn compute_release_status(&self, wt: WorkerType) -> Option<WorkerReleaseStatus> {
        let api_url = self.config.worker_api_url(wt);
        let cache_path = app_paths()
            .ok()?
            .version_cache_dir()
            .join(format!("{}.json", wt.desktop_bundle_product()));
        check_worker_release_status(
            api_url,
            wt.desktop_bundle_product(),
            APP_VERSION,
            &cache_path,
        )
        .ok()
    }

    fn refresh_status(&mut self, wt: WorkerType) {
        let status_file = match app_paths() {
            Ok(paths) => paths.worker_paths(wt).status_file,
            Err(e) => {
                self.last_error = Some(e.to_string());
                return;
            }
        };

        let snapshot = match fs::read(&status_file) {
            Ok(bytes) => serde_json::from_slice::<WorkerStatusSnapshot>(&bytes).ok(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => {
                self.last_error =
                    Some(format!("impossible de lire {}: {e}", status_file.display()));
                None
            }
        };

        self.state_mut(wt).status_snapshot = snapshot;
    }

    fn poll_child(&mut self, wt: WorkerType) {
        let state = self.state_mut(wt);
        let Some(child) = state.child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                state.last_message = Some(format!("worker arrete ({status})"));
                state.child = None;
            }
            Ok(None) => {}
            Err(e) => {
                state.last_message = Some(format!("erreur poll: {e}"));
            }
        }
    }

    pub fn state_mut(&mut self, wt: WorkerType) -> &mut WorkerUiState {
        match wt {
            WorkerType::RssScrapper => &mut self.rss,
            WorkerType::SourceEmbedding => &mut self.embedding,
        }
    }

    pub fn state(&self, wt: WorkerType) -> &WorkerUiState {
        match wt {
            WorkerType::RssScrapper => &self.rss,
            WorkerType::SourceEmbedding => &self.embedding,
        }
    }

    pub fn save_config(&mut self) {
        if let Err(e) = save_workers_config(&self.config_path, &self.config) {
            self.last_error = Some(format!("sauvegarde impossible: {e}"));
        } else {
            self.last_error = None;
            self.refresh_release_statuses();
            self.refresh_gpu_support();
        }
    }

    pub fn test_connection(&mut self, wt: WorkerType) {
        let (url, key) = self.api_credentials(wt);
        match check_worker_connection(url, key) {
            Ok(result) => {
                self.state_mut(wt).connection_check = Some(result);
                self.last_error = None;
            }
            Err(e) => {
                self.state_mut(wt).connection_check = Some(WorkerConnectionCheck {
                    ok: false,
                    worker_type: None,
                    worker_name: None,
                    checked_at: Utc::now(),
                    error: Some(e.to_string()),
                });
            }
        }
    }

    pub fn api_credentials(&self, wt: WorkerType) -> (&str, &str) {
        match wt {
            WorkerType::RssScrapper => (self.config.worker_api_url(wt), &self.config.rss.api_key),
            WorkerType::SourceEmbedding => (
                self.config.worker_api_url(wt),
                &self.config.embedding.api_key,
            ),
        }
    }

    pub fn binary_path(&self, wt: WorkerType) -> Option<PathBuf> {
        match wt {
            WorkerType::RssScrapper => self.config.rss.binary_path.clone(),
            WorkerType::SourceEmbedding => self.config.embedding.binary_path.clone(),
        }
    }

    pub fn service_mode(&self, wt: WorkerType) -> ServiceMode {
        match wt {
            WorkerType::RssScrapper => self.config.rss.service_mode,
            WorkerType::SourceEmbedding => self.config.embedding.service_mode,
        }
    }

    pub fn runtime_paths(
        &self,
        wt: WorkerType,
    ) -> Option<manifeed_worker_common::WorkerRuntimePaths> {
        app_paths().ok().map(|p| p.worker_paths(wt))
    }

    pub fn release_blocked(&self, wt: WorkerType) -> bool {
        self.state(wt)
            .release_status
            .as_ref()
            .map(|s| s.status == ReleaseCheckStatus::Incompatible)
            .unwrap_or(false)
    }

    pub fn refresh_gpu_support(&mut self) {
        let Some(binary) = self.binary_path(WorkerType::SourceEmbedding) else {
            self.gpu_support = None;
            return;
        };
        if !binary.exists() {
            self.gpu_support = Some(GpuSupport {
                error: Some(format!("binaire introuvable: {}", binary.display())),
                ..GpuSupport::default()
            });
            return;
        }
        self.gpu_support = Some(GpuSupport::probe(&binary, &self.config_path));
    }

    pub fn start_worker(&mut self, wt: WorkerType) {
        if self.release_blocked(wt) {
            self.last_error = Some(format!(
                "{}: version sous le minimum supporte, mise a jour requise.",
                wt.display_name()
            ));
            return;
        }

        if wt == WorkerType::SourceEmbedding
            && self.config.embedding.acceleration_mode == AccelerationMode::Gpu
        {
            let gpu = self.gpu_support.clone().unwrap_or_default();
            if !gpu.is_supported() {
                self.last_error = Some(format!("GPU indisponible: {}", gpu.summary()));
                return;
            }
        }

        if self.service_mode(wt) == ServiceMode::Background {
            match start_user_service(wt) {
                Ok(()) => {
                    self.state_mut(wt).last_message = Some("service demarre".to_string());
                    self.last_error = None;
                }
                Err(e) => self.last_error = Some(e.to_string()),
            }
            return;
        }

        if external_worker_running(self.state(wt).status_snapshot.as_ref())
            && self.state(wt).child.is_none()
        {
            self.last_error = Some(format!("un {} externe tourne deja", wt.display_name()));
            return;
        }

        let Some(binary) = self.binary_path(wt) else {
            self.last_error = Some(format!(
                "aucun binaire configure pour {}",
                wt.display_name()
            ));
            return;
        };
        if !binary.exists() {
            self.last_error = Some(format!("binaire introuvable: {}", binary.display()));
            return;
        }

        let Some(paths) = self.runtime_paths(wt) else {
            self.last_error = Some("chemins locaux introuvables".to_string());
            return;
        };
        if let Some(parent) = paths.log_file.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                self.last_error = Some(format!("dossier logs: {e}"));
                return;
            }
        }

        let stdout = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.log_file)
        {
            Ok(f) => f,
            Err(e) => {
                self.last_error = Some(format!("logs: {e}"));
                return;
            }
        };
        let stderr = match stdout.try_clone() {
            Ok(f) => f,
            Err(e) => {
                self.last_error = Some(format!("clone logs: {e}"));
                return;
            }
        };

        let mut cmd = Command::new(&binary);
        cmd.arg("run")
            .arg("--config")
            .arg(&self.config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));

        match cmd.spawn() {
            Ok(child) => {
                self.state_mut(wt).child = Some(child);
                self.state_mut(wt).last_message = Some("worker demarre".to_string());
                self.last_error = None;
            }
            Err(e) => {
                self.last_error = Some(format!("lancement impossible: {e}"));
            }
        }
    }

    pub fn stop_worker(&mut self, wt: WorkerType) {
        if self.service_mode(wt) == ServiceMode::Background {
            match stop_user_service(wt) {
                Ok(()) => {
                    self.state_mut(wt).last_message = Some("service arrete".to_string());
                    self.last_error = None;
                }
                Err(e) => self.last_error = Some(e.to_string()),
            }
            return;
        }

        if let Some(mut child) = self.state_mut(wt).child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.state_mut(wt).last_message = Some("worker arrete".to_string());
        }
    }

    pub fn restart_worker(&mut self, wt: WorkerType) {
        self.stop_worker(wt);
        self.start_worker(wt);
    }

    pub fn install_service(&mut self, wt: WorkerType) {
        let Some(binary) = self.binary_path(wt) else {
            self.last_error = Some(format!("aucun binaire pour {}", wt.display_name()));
            return;
        };
        match install_user_service(wt, &binary, &self.config_path) {
            Ok(()) => {
                match wt {
                    WorkerType::RssScrapper => {
                        self.config.rss.service_mode = ServiceMode::Background;
                    }
                    WorkerType::SourceEmbedding => {
                        self.config.embedding.service_mode = ServiceMode::Background;
                    }
                }
                self.save_config();
                self.state_mut(wt).last_message = Some("service installe".to_string());
            }
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    pub fn uninstall_service(&mut self, wt: WorkerType) {
        match uninstall_user_service(wt) {
            Ok(()) => {
                match wt {
                    WorkerType::RssScrapper => {
                        self.config.rss.service_mode = ServiceMode::Manual;
                    }
                    WorkerType::SourceEmbedding => {
                        self.config.embedding.service_mode = ServiceMode::Manual;
                    }
                }
                self.save_config();
                self.state_mut(wt).last_message = Some("service supprime".to_string());
            }
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    pub fn open_logs(&mut self, wt: WorkerType) {
        let Some(paths) = self.runtime_paths(wt) else {
            self.last_error = Some("chemins introuvables".to_string());
            return;
        };
        if let Err(e) = crate::helpers::open_path(&paths.log_file) {
            self.last_error = Some(e);
        }
    }

    pub fn stop_all_children(&mut self) {
        stop_child(&mut self.rss);
        stop_child(&mut self.embedding);
    }
}

fn stop_child(state: &mut WorkerUiState) {
    if let Some(mut child) = state.child.take() {
        let _ = child.kill();
        let _ = child.wait();
        state.last_message = Some("worker arrete".to_string());
    }
}
