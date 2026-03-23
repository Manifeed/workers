mod app;
mod gpu;
mod helpers;
mod pages;
mod state;
mod theme;
mod widgets;

use std::time::Duration;

use eframe::egui;
use manifeed_worker_common::WorkerType;

use app::{AppPage, ControlApp, APP_VERSION};
use state::summarize;

fn main() -> eframe::Result {
	let options = eframe::NativeOptions {
		viewport: egui::ViewportBuilder::default()
			.with_inner_size([1100.0, 720.0])
			.with_min_inner_size([860.0, 560.0]),
		..Default::default()
	};

	eframe::run_native(
		"Manifeed Workers",
		options,
		Box::new(|cc| {
			theme::configure(&cc.egui_ctx);
			Ok(Box::new(ControlApp::bootstrap()))
		}),
	)
}

impl eframe::App for ControlApp {
	fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
		if self.last_refresh.elapsed() >= Duration::from_secs(1) {
			self.refresh();
		}
		ctx.request_repaint_after(Duration::from_secs(1));

		let wt = match self.current_page {
			AppPage::Scraping => WorkerType::RssScrapper,
			AppPage::Embedding => WorkerType::SourceEmbedding,
		};

		draw_top_bar(ctx, wt);
		draw_sidebar(ctx, self);

		egui::CentralPanel::default()
			.frame(egui::Frame::default().fill(theme::LIGHT_BG))
			.show(ctx, |ui| {
				egui::ScrollArea::vertical()
					.auto_shrink([false, false])
					.show(ui, |ui| {
						if let Some(msg) = &self.last_error.clone() {
							widgets::banner(ui, theme::RED, msg);
						}
						self.draw_page(ui, wt);
					});
			});
	}

	fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
		self.stop_all_children();
	}
}

fn draw_top_bar(ctx: &egui::Context, wt: WorkerType) {
	egui::TopBottomPanel::top("top_bar")
		.frame(egui::Frame::default().fill(theme::DARK_BG))
		.show(ctx, |ui| {
			ui.add_space(10.0);
			ui.horizontal_wrapped(|ui| {
				ui.label(
					egui::RichText::new("Manifeed Workers")
						.size(24.0)
						.strong()
						.color(theme::DARK_TEXT),
				);
				ui.add_space(6.0);
				let page_label = match wt {
					WorkerType::RssScrapper => "Scraping",
					WorkerType::SourceEmbedding => "Embedding",
				};
				widgets::pill(ui, theme::worker_accent(wt), page_label);
				widgets::pill(
					ui,
					theme::DARK_TEXT_MUTED,
					&format!("v{APP_VERSION}"),
				);
			});
			ui.add_space(10.0);
		});
}

fn draw_sidebar(ctx: &egui::Context, app: &mut ControlApp) {
	egui::SidePanel::left("sidebar")
		.exact_width(220.0)
		.frame(egui::Frame::default().fill(theme::DARK_SURFACE))
		.show(ctx, |ui| {
			ui.add_space(14.0);
			ui.label(
				egui::RichText::new("Navigation")
					.size(11.0)
					.color(theme::DARK_TEXT_MUTED),
			);
			ui.add_space(6.0);

			let rss_resp = sidebar_nav(
				ui,
				app.current_page == AppPage::Scraping,
				"Scraping",
				app.rss.status_snapshot.as_ref(),
				theme::TEAL,
			);
			if rss_resp.interact(egui::Sense::click()).clicked() {
				app.current_page = AppPage::Scraping;
			}

			let emb_resp = sidebar_nav(
				ui,
				app.current_page == AppPage::Embedding,
				"Embedding",
				app.embedding.status_snapshot.as_ref(),
				theme::AMBER,
			);
			if emb_resp.interact(egui::Sense::click()).clicked() {
				app.current_page = AppPage::Embedding;
			}

			ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
				ui.add_space(10.0);
				ui.label(
					egui::RichText::new(app.config_path.display().to_string())
						.small()
						.color(theme::DARK_TEXT_MUTED),
				);
			});
		});
}

fn sidebar_nav(
	ui: &mut egui::Ui,
	selected: bool,
	label: &str,
	snapshot: Option<&manifeed_worker_common::WorkerStatusSnapshot>,
	accent: egui::Color32,
) -> egui::Response {
	let summary = summarize(snapshot);
	let fill = if selected {
		accent.gamma_multiply(0.15)
	} else {
		egui::Color32::TRANSPARENT
	};
	let stroke_color = if selected {
		accent
	} else {
		egui::Color32::from_rgba_premultiplied(255, 255, 255, 16)
	};

	let response = egui::Frame::group(ui.style())
		.fill(fill)
		.stroke(egui::Stroke::new(1.0, stroke_color))
		.corner_radius(10.0)
		.inner_margin(egui::Margin::same(10))
		.show(ui, |ui| {
			ui.set_min_width(180.0);
			ui.label(
				egui::RichText::new(label)
					.strong()
					.size(16.0)
					.color(if selected {
						theme::DARK_TEXT
					} else {
						theme::DARK_TEXT_MUTED
					}),
			);
			ui.horizontal_wrapped(|ui| {
				widgets::pill(
					ui,
					theme::phase_color(&summary.phase),
					&summary.phase,
				);
				widgets::pill(
					ui,
					egui::Color32::from_rgb(120, 145, 158),
					&summary.connection,
				);
			});
		})
		.response;

	ui.add_space(4.0);
	response
}
