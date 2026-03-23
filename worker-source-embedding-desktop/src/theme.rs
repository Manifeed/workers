use eframe::egui;
use manifeed_worker_common::WorkerType;

// Dark palette (top bar, sidebar)
pub const DARK_BG: egui::Color32 = egui::Color32::from_rgb(14, 28, 38);
pub const DARK_SURFACE: egui::Color32 = egui::Color32::from_rgb(18, 38, 52);
pub const DARK_TEXT: egui::Color32 = egui::Color32::from_rgb(240, 238, 232);
pub const DARK_TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(150, 172, 185);

// Light palette (central panel, cards)
pub const LIGHT_BG: egui::Color32 = egui::Color32::from_rgb(244, 241, 235);
pub const CARD_BG: egui::Color32 = egui::Color32::from_rgb(253, 251, 248);
pub const CARD_BORDER: egui::Color32 = egui::Color32::from_rgb(215, 208, 196);
pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(28, 33, 38);
pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(95, 102, 112);

// Worker accents
pub const TEAL: egui::Color32 = egui::Color32::from_rgb(24, 100, 108);
pub const AMBER: egui::Color32 = egui::Color32::from_rgb(176, 112, 54);

// Semantic colors
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(34, 120, 72);
pub const RED: egui::Color32 = egui::Color32::from_rgb(180, 50, 40);
pub const ORANGE: egui::Color32 = egui::Color32::from_rgb(200, 130, 35);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(102, 111, 118);

pub fn configure(ctx: &egui::Context) {
	let mut style = (*ctx.style()).clone();
	style.spacing.item_spacing = egui::vec2(12.0, 10.0);
	style.spacing.button_padding = egui::vec2(14.0, 8.0);
	style.visuals.widgets.inactive.corner_radius = 10.0.into();
	style.visuals.widgets.active.corner_radius = 10.0.into();
	style.visuals.widgets.hovered.corner_radius = 10.0.into();
	style.visuals.window_corner_radius = 14.0.into();
	style.visuals.panel_fill = LIGHT_BG;
	style.visuals.window_fill = LIGHT_BG;
	style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(238, 233, 225);
	style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(228, 221, 210);
	style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(214, 205, 192);
	style.visuals.selection.bg_fill = TEAL;
	ctx.set_style(style);
}

pub fn worker_accent(wt: WorkerType) -> egui::Color32 {
	match wt {
		WorkerType::RssScrapper => TEAL,
		WorkerType::SourceEmbedding => AMBER,
	}
}

pub fn phase_color(phase: &str) -> egui::Color32 {
	match phase {
		"idle" => GREEN,
		"processing" => ORANGE,
		"error" => RED,
		"starting" => TEAL,
		"stopped" => MUTED,
		_ => TEXT_SECONDARY,
	}
}
