//! The gallery's pages. Each has a `State`, a `show` function drawing the
//! page and a `playground` function drawing its live options.

use egui::{Color32, CornerRadius, Sense, Stroke, Ui, vec2};
use egui_pop::theme::Tokens;

pub mod builtins;
pub mod overview;
pub mod tokens;

/// A section title with the theme's section spacing above it.
pub(crate) fn section(ui: &mut Ui, title: &str) {
    let t = Tokens::of(ui.ctx());
    ui.add_space(t.space.l);
    ui.strong(title);
    ui.add_space(t.space.xs);
}

/// A filled swatch with an outline, `size` points wide.
pub(crate) fn swatch(ui: &mut Ui, fill: Color32, size: egui::Vec2) -> egui::Response {
    let t = Tokens::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(t.radius.widget.round() as u8),
        fill,
        Stroke::new(1.0, t.border.default),
        egui::StrokeKind::Inside,
    );
    response
}

/// A small square swatch for tables.
pub(crate) fn chip(ui: &mut Ui, fill: Color32) -> egui::Response {
    let h = ui.spacing().interact_size.y * 0.8;
    swatch(ui, fill, vec2(h * 1.6, h))
}
