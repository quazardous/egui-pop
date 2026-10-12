//! The gallery's pages, each a plugin contributing a story. The theme
//! pages are plain functions over a state (`show` and `playground`),
//! reused by the tests as fixtures.

use egui::{Color32, CornerRadius, Sense, Stroke, Ui, vec2};
use egui_pop::theme::Tokens;

pub mod builtins;
pub mod debug;
pub mod dialogs;
pub mod frame;
pub mod icons;
pub mod menus;
pub mod overview;
pub mod settings;
pub mod story;
pub mod toasts;
pub mod tokens;
pub mod values;
pub mod viewport;
pub mod zones;

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
