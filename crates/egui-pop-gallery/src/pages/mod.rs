//! The gallery's pages, each a plugin contributing a story. The theme
//! pages are plain functions over a state (`show` and `playground`),
//! reused by the tests as fixtures.

use egui::{Color32, CornerRadius, Sense, Stroke, Ui, vec2};
use egui_pop::core::ui::{Space, UiPop};
use egui_pop::theme::Tokens;

pub mod arrangements;
pub mod builtins;
pub mod clipboard;
pub mod contexts;
pub mod cursors;
pub mod debug;
pub mod dialogs;
pub mod display;
pub mod editors;
pub mod files;
pub mod fonts;
pub mod frame;
pub mod grading;
pub mod graph_file;
pub mod history;
pub mod i18n;
pub mod icons;
pub mod images;
pub mod indicators;
pub mod inputs;
pub mod jobs;
pub mod layout;
pub mod library;
pub mod menus;
pub mod motion;
pub mod navigator;
pub mod node_graph;
pub mod overview;
pub mod palettes;
pub mod placeholders;
pub mod platform;
pub mod pointer;
pub mod preview;
pub mod progress;
pub mod roles;
pub mod scale;
pub mod scoped;
pub mod sections;
pub mod settings;
pub mod sliders;
pub mod story;
pub mod strips;
pub mod table;
pub mod terminal;
pub mod theme_editor;
pub mod tips;
pub mod toasts;
pub mod tokens;
pub mod tree;
pub mod values;
pub mod viewport;
pub mod workbench;
pub mod zones;

/// A section title with the theme's section spacing above it.
pub(crate) fn section(ui: &mut Ui, title: &str) {
    ui.space_role(Space::Section);
    ui.section_title(title);
    ui.add_space(Tokens::of(ui.ctx()).space.xs);
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
