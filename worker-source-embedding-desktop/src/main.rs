use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use eframe::egui;
use serde::Deserialize;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 420.0])
            .with_min_inner_size([520.0, 320.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Manifeed Embedding Worker",
        options,
        Box::new(|_cc| Ok(Box::new(ControlApp::bootstrap()))),
    )
}

#[derive(Clone, Debug, Deserialize)]
struct CurrentTaskSnapshot {
    task_id: u64,
    execution_id: u64,
    job_id: String,
    model_name: String,
    source_count: usize,
    started_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct NetworkTotalsSnapshot {
    bytes_sent: u64,
    bytes_received: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct WorkerStatusSnapshot {
    pid: u32,
    phase: String,
    server_connection: String,
    execution_backend: String,
    last_updated_at: DateTime<Utc>,
    last_server_contact_at: Option<DateTime<Utc>>,
    current_task: Option<CurrentTaskSnapshot>,
    completed_task_count: u64,
    network_totals: NetworkTotalsSnapshot,
    last_error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct CpuSample {
    process_ticks: u64,
    total_ticks: u64,
}

struct ControlApp {
    worker_binary: PathBuf,
    env_file: PathBuf,
    status_file: PathBuf,
    log_file: PathBuf,
    api_url: Option<String>,
    child: Option<Child>,
    status_snapshot: Option<WorkerStatusSnapshot>,
    bandwidth_up_bps: f64,
    bandwidth_down_bps: f64,
    last_network_sample: Option<(Instant, u64, u64)>,
    cpu_usage_percent: f64,
    last_cpu_sample: Option<CpuSample>,
    last_refresh_at: Instant,
    last_ui_error: Option<String>,
}

impl ControlApp {
    fn bootstrap() -> Self {
        let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        let install_dir = current_exe.parent().unwrap_or(Path::new(".")).to_path_buf();
        let env_file = default_env_file();
        let env_vars = read_env_file(&env_file).unwrap_or_default();
        let worker_binary = install_dir.join("worker-source-embedding");
        let status_file = env_vars
            .get("MANIFEED_EMBEDDING_STATUS_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(default_status_file);
        let log_file = default_log_file();

        Self {
            worker_binary,
            env_file,
            status_file,
            log_file,
            api_url: env_vars.get("MANIFEED_API_URL").cloned(),
            child: None,
            status_snapshot: None,
            bandwidth_up_bps: 0.0,
            bandwidth_down_bps: 0.0,
            last_network_sample: None,
            cpu_usage_percent: 0.0,
            last_cpu_sample: None,
            last_refresh_at: Instant::now() - Duration::from_secs(5),
            last_ui_error: None,
        }
    }

    fn start_worker(&mut self) {
        if self.child.is_some() {
            return;
        }
        if external_worker_running(self.status_snapshot.as_ref()) {
            self.last_ui_error = Some(
                "un worker externe est deja actif; arrete-le avant de lancer celui de l'interface"
                    .to_string(),
            );
            return;
        }

        let env_vars = match read_env_file(&self.env_file) {
            Ok(vars) => vars,
            Err(error) => {
                self.last_ui_error = Some(format!(
                    "impossible de lire le fichier d'environnement {}: {error}",
                    self.env_file.display()
                ));
                return;
            }
        };

        if !self.worker_binary.exists() {
            self.last_ui_error = Some(format!(
                "binaire worker introuvable: {}",
                self.worker_binary.display()
            ));
            return;
        }

        if let Some(parent) = self.log_file.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                self.last_ui_error = Some(format!(
                    "impossible de creer le dossier de logs {}: {error}",
                    parent.display()
                ));
                return;
            }
        }

        let stdout = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
        {
            Ok(file) => file,
            Err(error) => {
                self.last_ui_error = Some(format!(
                    "impossible d'ouvrir le fichier de logs {}: {error}",
                    self.log_file.display()
                ));
                return;
            }
        };
        let stderr = match stdout.try_clone() {
            Ok(file) => file,
            Err(error) => {
                self.last_ui_error = Some(format!(
                    "impossible de dupliquer le fichier de logs {}: {error}",
                    self.log_file.display()
                ));
                return;
            }
        };

        let mut command = Command::new(&self.worker_binary);
        command.arg("run");
        command.stdin(Stdio::null());
        command.stdout(Stdio::from(stdout));
        command.stderr(Stdio::from(stderr));
        for (key, value) in env_vars {
            command.env(key, value);
        }

        match command.spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.last_ui_error = None;
                self.last_cpu_sample = None;
            }
            Err(error) => {
                self.last_ui_error = Some(format!(
                    "impossible de lancer le worker {}: {error}",
                    self.worker_binary.display()
                ));
            }
        }
    }

    fn stop_worker(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.cpu_usage_percent = 0.0;
        self.bandwidth_up_bps = 0.0;
        self.bandwidth_down_bps = 0.0;
        self.last_cpu_sample = None;
        self.last_network_sample = None;
    }

    fn restart_worker(&mut self) {
        self.stop_worker();
        self.start_worker();
    }

    fn refresh(&mut self) {
        self.poll_child();
        self.refresh_status_file();
        self.refresh_network_rates();
        self.refresh_cpu_usage();
        self.last_refresh_at = Instant::now();
    }

    fn poll_child(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(exit_status)) => {
                self.last_ui_error = Some(format!("le worker s'est arrete ({exit_status})"));
                self.child = None;
                self.cpu_usage_percent = 0.0;
            }
            Ok(None) => {}
            Err(error) => {
                self.last_ui_error = Some(format!("impossible de verifier le worker: {error}"));
            }
        }
    }

    fn refresh_status_file(&mut self) {
        let bytes = match fs::read(&self.status_file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => {
                self.last_ui_error = Some(format!(
                    "impossible de lire le statut {}: {error}",
                    self.status_file.display()
                ));
                return;
            }
        };

        match serde_json::from_slice::<WorkerStatusSnapshot>(&bytes) {
            Ok(snapshot) => self.status_snapshot = Some(snapshot),
            Err(error) => {
                self.last_ui_error = Some(format!(
                    "impossible de parser le statut {}: {error}",
                    self.status_file.display()
                ));
            }
        }
    }

    fn refresh_network_rates(&mut self) {
        let Some(snapshot) = self.status_snapshot.as_ref() else {
            return;
        };
        let now = Instant::now();
        let current_sent = snapshot.network_totals.bytes_sent;
        let current_received = snapshot.network_totals.bytes_received;

        if let Some((last_at, last_sent, last_received)) = self.last_network_sample {
            let elapsed = now.saturating_duration_since(last_at).as_secs_f64();
            if elapsed > 0.0 {
                self.bandwidth_up_bps =
                    current_sent.saturating_sub(last_sent) as f64 / elapsed;
                self.bandwidth_down_bps =
                    current_received.saturating_sub(last_received) as f64 / elapsed;
            }
        }

        self.last_network_sample = Some((now, current_sent, current_received));
    }

    fn refresh_cpu_usage(&mut self) {
        let pid = self.child.as_ref().map(|child| child.id()).or_else(|| {
            self.status_snapshot
                .as_ref()
                .map(|snapshot| snapshot.pid)
                .filter(|pid| process_exists(*pid))
        });

        let Some(pid) = pid else {
            self.cpu_usage_percent = 0.0;
            self.last_cpu_sample = None;
            return;
        };

        let process_ticks = match read_process_ticks(pid) {
            Some(value) => value,
            None => {
                self.cpu_usage_percent = 0.0;
                self.last_cpu_sample = None;
                return;
            }
        };
        let total_ticks = match read_total_cpu_ticks() {
            Some(value) => value,
            None => return,
        };

        if let Some(previous) = self.last_cpu_sample {
            let delta_process = process_ticks.saturating_sub(previous.process_ticks) as f64;
            let delta_total = total_ticks.saturating_sub(previous.total_ticks) as f64;
            if delta_total > 0.0 {
                let cpu_count = std::thread::available_parallelism()
                    .map(|value| value.get() as f64)
                    .unwrap_or(1.0);
                self.cpu_usage_percent = (delta_process / delta_total) * cpu_count * 100.0;
            }
        }
        self.last_cpu_sample = Some(CpuSample {
            process_ticks,
            total_ticks,
        });
    }

    fn effective_pid(&self) -> Option<u32> {
        self.child
            .as_ref()
            .map(|child| child.id())
            .or_else(|| self.status_snapshot.as_ref().map(|snapshot| snapshot.pid))
    }
}

impl eframe::App for ControlApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_refresh_at.elapsed() >= Duration::from_secs(1) {
            self.refresh();
        }
        ctx.request_repaint_after(Duration::from_secs(1));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Manifeed Embedding Worker");
            ui.label(self.api_url.as_deref().unwrap_or("API inconnue"));
            ui.separator();

            let external_running = external_worker_running(self.status_snapshot.as_ref());
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.child.is_none() && !external_running,
                        egui::Button::new("Demarrer"),
                    )
                    .clicked()
                {
                    self.start_worker();
                }
                if ui
                    .add_enabled(self.child.is_some(), egui::Button::new("Arreter"))
                    .clicked()
                {
                    self.stop_worker();
                }
                if ui
                    .add_enabled(self.child.is_some(), egui::Button::new("Redemarrer"))
                    .clicked()
                {
                    self.restart_worker();
                }
            });

            if external_running && self.child.is_none() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Un worker externe tourne deja. L'interface ne le pilote pas.",
                );
            }
            if let Some(message) = &self.last_ui_error {
                ui.colored_label(egui::Color32::RED, message);
            }

            ui.separator();
            egui::Grid::new("worker_metrics")
                .num_columns(2)
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    let phase = self
                        .status_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.phase.as_str())
                        .unwrap_or("inconnu");
                    ui.label("Etat");
                    ui.label(phase);
                    ui.end_row();

                    let connection = self
                        .status_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.server_connection.as_str())
                        .unwrap_or("inconnu");
                    ui.label("Connexion serveur");
                    ui.label(connection);
                    ui.end_row();

                    ui.label("PID");
                    ui.label(
                        self.effective_pid()
                            .map(|pid| pid.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                    );
                    ui.end_row();

                    ui.label("Backend");
                    ui.label(
                        self.status_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.execution_backend.clone())
                            .unwrap_or_else(|| "-".to_string()),
                    );
                    ui.end_row();

                    ui.label("CPU");
                    ui.label(format!("{:.1} %", self.cpu_usage_percent.max(0.0)));
                    ui.end_row();

                    ui.label("Debit sortant");
                    ui.label(format!("{}/s", format_bytes(self.bandwidth_up_bps)));
                    ui.end_row();

                    ui.label("Debit entrant");
                    ui.label(format!("{}/s", format_bytes(self.bandwidth_down_bps)));
                    ui.end_row();

                    ui.label("Taches terminees");
                    ui.label(
                        self.status_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.completed_task_count.to_string())
                            .unwrap_or_else(|| "0".to_string()),
                    );
                    ui.end_row();

                    ui.label("Dernier contact serveur");
                    ui.label(
                        self.status_snapshot
                            .as_ref()
                            .and_then(|snapshot| snapshot.last_server_contact_at)
                            .map(format_datetime)
                            .unwrap_or_else(|| "-".to_string()),
                    );
                    ui.end_row();
                });

            ui.separator();
            ui.heading("Tache courante");
            if let Some(task) = self
                .status_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.current_task.as_ref())
            {
                ui.label(format!("Task id: {}", task.task_id));
                ui.label(format!("Execution id: {}", task.execution_id));
                ui.label(format!("Job: {}", task.job_id));
                ui.label(format!("Modele: {}", task.model_name));
                ui.label(format!("Sources: {}", task.source_count));
                ui.label(format!("Debut: {}", format_datetime(task.started_at)));
            } else {
                ui.label("Aucune tache en cours.");
            }

            ui.separator();
            ui.label(format!("Statut local: {}", self.status_file.display()));
            ui.label(format!("Logs: {}", self.log_file.display()));
            if let Some(snapshot) = &self.status_snapshot {
                if let Some(last_error) = &snapshot.last_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("Derniere erreur: {last_error}"));
                }
                ui.label(format!(
                    "Derniere mise a jour: {}",
                    format_datetime(snapshot.last_updated_at)
                ));
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_worker();
    }
}

fn default_env_file() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".config/manifeed/worker-source-embedding.env")
}

fn default_status_file() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".local/state/manifeed/worker-source-embedding/status.json")
}

fn default_log_file() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".cache/manifeed/worker-source-embedding/worker.log")
}

fn read_env_file(path: &Path) -> io::Result<HashMap<String, String>> {
    let contents = fs::read_to_string(path)?;
    let mut vars = HashMap::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        vars.insert(key.trim().to_string(), value.trim().to_string());
    }
    Ok(vars)
}

fn external_worker_running(snapshot: Option<&WorkerStatusSnapshot>) -> bool {
    snapshot
        .map(|snapshot| process_exists(snapshot.pid) && snapshot.phase != "stopped")
        .unwrap_or(false)
}

fn process_exists(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}

fn read_process_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    let remainder = stat.get(end + 2..)?;
    let fields = remainder.split_whitespace().collect::<Vec<_>>();
    let utime = fields.get(11)?.parse::<u64>().ok()?;
    let stime = fields.get(12)?.parse::<u64>().ok()?;
    Some(utime.saturating_add(stime))
}

fn read_total_cpu_ticks() -> Option<u64> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let cpu = stat.lines().next()?;
    let mut total = 0_u64;
    for value in cpu.split_whitespace().skip(1) {
        total = total.saturating_add(value.parse::<u64>().ok()?);
    }
    Some(total)
}

fn format_bytes(bytes_per_second: f64) -> String {
    let value = bytes_per_second.max(0.0);
    if value >= 1024.0 * 1024.0 {
        format!("{:.2} MiB", value / (1024.0 * 1024.0))
    } else if value >= 1024.0 {
        format!("{:.1} KiB", value / 1024.0)
    } else {
        format!("{value:.0} B")
    }
}

fn format_datetime(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}
