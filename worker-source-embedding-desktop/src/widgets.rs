use eframe::egui;
use manifeed_worker_common::ServiceMode;

use crate::theme;

pub fn pill(ui: &mut egui::Ui, color: egui::Color32, label: &str) {
	egui::Frame::group(ui.style())
		.fill(color.gamma_multiply(0.14))
		.stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.5)))
		.corner_radius(999.0)
		.inner_margin(egui::Margin::symmetric(9, 3))
		.show(ui, |ui| {
			ui.label(egui::RichText::new(label).color(color).strong().small());
		});
}

pub fn primary_button(label: &str) -> egui::Button<'_> {
	egui::Button::new(
		egui::RichText::new(label)
			.strong()
			.color(theme::DARK_TEXT),
	)
	.fill(theme::TEAL)
	.stroke(egui::Stroke::NONE)
	.min_size(egui::vec2(110.0, 34.0))
}

pub fn secondary_button(label: &str) -> egui::Button<'_> {
	egui::Button::new(
		egui::RichText::new(label)
			.strong()
			.color(theme::TEXT_PRIMARY),
	)
	.fill(egui::Color32::from_rgb(236, 230, 220))
	.stroke(egui::Stroke::new(1.0, theme::CARD_BORDER))
	.min_size(egui::vec2(110.0, 34.0))
}

pub fn section_card(
	ui: &mut egui::Ui,
	title: &str,
	subtitle: &str,
	add_contents: impl FnOnce(&mut egui::Ui),
) {
	egui::Frame::group(ui.style())
		.fill(theme::CARD_BG)
		.stroke(egui::Stroke::new(1.0, theme::CARD_BORDER))
		.corner_radius(12.0)
		.inner_margin(egui::Margin::same(16))
		.show(ui, |ui| {
			ui.heading(title);
			ui.label(
				egui::RichText::new(subtitle).color(theme::TEXT_SECONDARY),
			);
			ui.add_space(8.0);
			add_contents(ui);
		});
}

pub fn banner(ui: &mut egui::Ui, color: egui::Color32, message: &str) {
	egui::Frame::group(ui.style())
		.fill(color.gamma_multiply(0.1))
		.stroke(egui::Stroke::new(1.0, color))
		.corner_radius(8.0)
		.inner_margin(egui::Margin::same(10))
		.show(ui, |ui| {
			ui.colored_label(color, message);
		});
	ui.add_space(6.0);
}

pub fn service_mode_selector(ui: &mut egui::Ui, mode: &mut ServiceMode) {
	egui::ComboBox::from_id_salt(ui.next_auto_id())
		.selected_text(service_mode_label(*mode))
		.show_ui(ui, |ui| {
			ui.selectable_value(mode, ServiceMode::Manual, "Manuel");
			ui.selectable_value(mode, ServiceMode::Background, "Service utilisateur");
		});
}

pub fn service_mode_label(mode: ServiceMode) -> &'static str {
	match mode {
		ServiceMode::Manual => "Manuel",
		ServiceMode::Background => "Service utilisateur",
	}
}

pub fn compact_mode_label(mode: ServiceMode) -> &'static str {
	match mode {
		ServiceMode::Manual => "manuel",
		ServiceMode::Background => "service",
	}
}

pub fn runtime_path_row(ui: &mut egui::Ui, label: &str, value: &str) {
	ui.label(
		egui::RichText::new(label)
			.small()
			.color(theme::TEXT_SECONDARY),
	);
	ui.monospace(value);
	ui.add_space(4.0);
}
