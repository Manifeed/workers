use eframe::egui;
use manifeed_worker_common::{AccelerationMode, ReleaseCheckStatus, ServiceMode, WorkerType};

use crate::app::ControlApp;
use crate::helpers::format_datetime;
use crate::state::summarize;
use crate::theme;
use crate::widgets;

impl ControlApp {
    pub fn draw_page(&mut self, ui: &mut egui::Ui, wt: WorkerType) {
        self.draw_status_strip(ui, wt);
        ui.add_space(12.0);
        self.draw_config_section(ui, wt);
        ui.add_space(10.0);
        self.draw_service_section(ui, wt);
        ui.add_space(10.0);
        self.draw_runtime_section(ui, wt);
    }

    fn draw_status_strip(&mut self, ui: &mut egui::Ui, wt: WorkerType) {
        let summary = summarize(self.state(wt).status_snapshot.as_ref());
        let accent = theme::worker_accent(wt);
        let release = self.state(wt).release_status.clone();

        egui::Frame::group(ui.style())
            .fill(accent.gamma_multiply(0.07))
            .stroke(egui::Stroke::new(1.0, accent.gamma_multiply(0.35)))
            .corner_radius(12.0)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    widgets::pill(ui, theme::phase_color(&summary.phase), &summary.phase);
                    widgets::pill(
                        ui,
                        egui::Color32::from_rgb(88, 113, 129),
                        &summary.connection,
                    );
                    widgets::pill(
                        ui,
                        theme::TEXT_SECONDARY,
                        &format!("{} taches", summary.completed_tasks),
                    );
                    widgets::pill(
                        ui,
                        accent,
                        &format!(
                            "mode {}",
                            widgets::compact_mode_label(self.service_mode(wt))
                        ),
                    );
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let blocked = self.release_blocked(wt);
                    if ui
                        .add_enabled(!blocked, widgets::primary_button("Demarrer"))
                        .clicked()
                    {
                        self.start_worker(wt);
                    }
                    if ui.add(widgets::secondary_button("Arreter")).clicked() {
                        self.stop_worker(wt);
                    }
                    if ui
                        .add_enabled(!blocked, widgets::secondary_button("Redemarrer"))
                        .clicked()
                    {
                        self.restart_worker(wt);
                    }
                    if ui.add(widgets::secondary_button("Ouvrir logs")).clicked() {
                        self.open_logs(wt);
                    }
                });

                if let Some(msg) = &self.state(wt).last_message.clone() {
                    ui.add_space(4.0);
                    ui.small(msg);
                }
            });

        if let Some(msg) = release_banner_message(release.as_ref(), wt) {
            ui.add_space(6.0);
            widgets::banner(ui, release_banner_color(release.as_ref()), &msg);
        }
    }

    fn draw_config_section(&mut self, ui: &mut egui::Ui, wt: WorkerType) {
        let subtitle = match wt {
            WorkerType::RssScrapper => "Cle API, mode d'execution et parallelisme RSS.",
            WorkerType::SourceEmbedding => {
                "Cle API, acceleration, mode d'execution et batch embedding."
            }
        };

        widgets::section_card(ui, "Configuration", subtitle, |ui| {
            self.draw_config_form(ui, wt);
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add(widgets::primary_button("Sauvegarder")).clicked() {
                    match wt {
                        WorkerType::RssScrapper => {
                            self.config.rss.enabled = true;
                        }
                        WorkerType::SourceEmbedding => {
                            self.config.embedding.enabled = true;
                        }
                    }
                    self.save_config();
                }
                if ui
                    .add(widgets::secondary_button("Tester la connexion"))
                    .clicked()
                {
                    self.test_connection(wt);
                }
                if wt == WorkerType::SourceEmbedding
                    && ui
                        .add(widgets::secondary_button("Verifier runtime"))
                        .clicked()
                {
                    self.refresh_gpu_support();
                }
            });
            self.draw_gpu_status(ui, wt);
            self.draw_connection_result(ui, wt);
        });
    }

    fn draw_config_form(&mut self, ui: &mut egui::Ui, wt: WorkerType) {
        let (api_key, mode) = match wt {
            WorkerType::RssScrapper => (
                &mut self.config.rss.api_key,
                &mut self.config.rss.service_mode,
            ),
            WorkerType::SourceEmbedding => (
                &mut self.config.embedding.api_key,
                &mut self.config.embedding.service_mode,
            ),
        };

        egui::Grid::new(ui.next_auto_id())
            .num_columns(2)
            .spacing(egui::vec2(16.0, 10.0))
            .show(ui, |ui| {
                ui.label("Cle API");
                ui.add(egui::TextEdit::singleline(api_key).desired_width(ui.available_width()));
                ui.end_row();

                ui.label("Mode d'execution");
                widgets::service_mode_selector(ui, mode);
                ui.end_row();

                match wt {
                    WorkerType::RssScrapper => {
                        ui.label("Max in flight requests");
                        ui.add(
                            egui::DragValue::new(&mut self.config.rss.max_in_flight_requests)
                                .range(1..=256)
                                .speed(1.0),
                        );
                        ui.end_row();
                    }
                    WorkerType::SourceEmbedding => {
                        ui.label("Inference batch size");
                        ui.add(
                            egui::DragValue::new(&mut self.config.embedding.inference_batch_size)
                                .range(1..=256)
                                .speed(1.0),
                        );
                        ui.end_row();
                    }
                }
            });

        if wt == WorkerType::SourceEmbedding {
            ui.add_space(6.0);
            let gpu_ok = self
                .gpu_support
                .as_ref()
                .map(|g| g.is_supported())
                .unwrap_or(false);
            ui.horizontal_wrapped(|ui| {
                ui.label("Acceleration");
                ui.selectable_value(
                    &mut self.config.embedding.acceleration_mode,
                    AccelerationMode::Auto,
                    "Auto",
                );
                ui.selectable_value(
                    &mut self.config.embedding.acceleration_mode,
                    AccelerationMode::Cpu,
                    "CPU",
                );
                ui.add_enabled_ui(gpu_ok, |ui| {
                    ui.selectable_value(
                        &mut self.config.embedding.acceleration_mode,
                        AccelerationMode::Gpu,
                        "GPU",
                    );
                });
            });
        }
    }

    fn draw_gpu_status(&self, ui: &mut egui::Ui, wt: WorkerType) {
        if wt != WorkerType::SourceEmbedding {
            return;
        }
        let Some(gpu) = &self.gpu_support else {
            return;
        };
        ui.add_space(6.0);
        let color = if gpu.is_supported() {
            theme::GREEN
        } else {
            theme::ORANGE
        };
        ui.colored_label(color, gpu.summary());
        let mut idx = 0;
        while idx < gpu.notes.len() && idx < 2 {
            ui.small(&gpu.notes[idx]);
            idx += 1;
        }
    }

    fn draw_runtime_section(&self, ui: &mut egui::Ui, wt: WorkerType) {
        let Some(paths) = self.runtime_paths(wt) else {
            return;
        };
        widgets::section_card(
            ui,
            "Runtime local",
            "Chemins, tache courante et diagnostics.",
            |ui| {
                widgets::runtime_path_row(ui, "Config", &self.config_path.display().to_string());
                widgets::runtime_path_row(ui, "Logs", &paths.log_file.display().to_string());
                widgets::runtime_path_row(ui, "Status", &paths.status_file.display().to_string());
                if wt == WorkerType::SourceEmbedding {
                    widgets::runtime_path_row(
                        ui,
                        "Cache modele",
                        &paths.cache_dir.display().to_string(),
                    );
                    if let Some(gpu) = &self.gpu_support {
                        if let Some(bundle) = &gpu.recommended_runtime_bundle {
                            ui.label(format!("Runtime recommande: {bundle}"));
                        }
                    }
                }
                if let Some(snap) = &self.state(wt).status_snapshot {
                    ui.add_space(6.0);
                    ui.small(format!(
                        "Mis a jour: {}",
                        format_datetime(snap.last_updated_at)
                    ));
                    if let Some(task) = &snap.current_task {
                        ui.small(format!(
                            "Tache: {}",
                            task.label
                                .clone()
                                .unwrap_or_else(|| format!("#{}", task.task_id))
                        ));
                    }
                    if let Some(err) = &snap.last_error {
                        ui.colored_label(theme::RED, format!("Erreur: {err}"));
                    }
                }
            },
        );
    }

    fn draw_service_section(&mut self, ui: &mut egui::Ui, wt: WorkerType) {
        widgets::section_card(
            ui,
            "Service systeme",
            "Gestion du service OS pour une execution continue.",
            |ui| {
                let mode = self.service_mode(wt);
                ui.horizontal_wrapped(|ui| {
                    let color = if mode == ServiceMode::Background {
                        theme::GREEN
                    } else {
                        theme::TEXT_SECONDARY
                    };
                    widgets::pill(ui, color, widgets::service_mode_label(mode));
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add(widgets::secondary_button("Installer")).clicked() {
                        self.install_service(wt);
                    }
                    if ui.add(widgets::secondary_button("Supprimer")).clicked() {
                        self.uninstall_service(wt);
                    }
                    if ui
                        .add(widgets::secondary_button("Verifier version"))
                        .clicked()
                    {
                        self.state_mut(wt).release_status = self.compute_release_status(wt);
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if wt == WorkerType::SourceEmbedding
                        && ui.add(widgets::secondary_button("Probe GPU")).clicked()
                    {
                        self.refresh_gpu_support();
                    }
                    if let Some(release) = &self.state(wt).release_status.clone() {
                        if let Some(manifest) = &release.manifest {
                            let url = manifest.download_url.clone();
                            if ui.add(widgets::secondary_button("Mise a jour")).clicked() {
                                let _ = crate::helpers::open_url(&url);
                            }
                        }
                    }
                });
            },
        );
    }

    fn draw_connection_result(&self, ui: &mut egui::Ui, wt: WorkerType) {
        let Some(conn) = &self.state(wt).connection_check else {
            return;
        };
        ui.add_space(6.0);
        let (color, text) = if conn.ok {
            (
                theme::GREEN,
                format!(
                    "Connexion OK: {}",
                    conn.worker_name.as_deref().unwrap_or("worker")
                ),
            )
        } else {
            (
                theme::RED,
                format!(
                    "Connexion echouee: {}",
                    conn.error.as_deref().unwrap_or("erreur inconnue")
                ),
            )
        };
        ui.colored_label(color, text);
    }
}

fn release_banner_message(
    release: Option<&manifeed_worker_common::WorkerReleaseStatus>,
    wt: WorkerType,
) -> Option<String> {
    let r = release?;
    let label = match wt {
        WorkerType::RssScrapper => "Scraping",
        WorkerType::SourceEmbedding => "Embedding",
    };
    match r.status {
        ReleaseCheckStatus::UpToDate => None,
        ReleaseCheckStatus::UpdateAvailable => Some(format!("{label}: mise a jour disponible.")),
        ReleaseCheckStatus::Incompatible => Some(format!("{label}: version locale incompatible.")),
        ReleaseCheckStatus::Unverified => r.message.as_ref().map(|m| format!("{label}: {m}")),
    }
}

fn release_banner_color(
    release: Option<&manifeed_worker_common::WorkerReleaseStatus>,
) -> egui::Color32 {
    match release.map(|r| r.status) {
        Some(ReleaseCheckStatus::UpdateAvailable) => theme::ORANGE,
        Some(ReleaseCheckStatus::Incompatible) => theme::RED,
        _ => theme::TEAL,
    }
}
