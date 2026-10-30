//! The theme at a glance.

use egui::{Align2, Color32, CornerRadius, FontId, Stroke, StrokeKind, Ui, vec2};
use egui_pop::core::kit::{self, Kit};
use egui_pop::core::widgets;
use egui_pop::theme::Tokens;
use egui_pop::tokens::contrast::{self, Level};

use super::{section, swatch};

/// The overview's options.
#[derive(Clone, Debug)]
pub struct State {
    /// Show token paths under the swatches.
    pub show_paths: bool,
    /// Show the contrast table.
    pub show_contrast: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            show_paths: true,
            show_contrast: true,
        }
    }
}

pub fn playground(ui: &mut Ui, state: &mut State) {
    widgets::checkbox(ui, &mut state.show_paths, "Token paths");
    widgets::checkbox(ui, &mut state.show_contrast, "Contrast checks");
}

/// A swatch with its name, and its path when asked.
fn named(ui: &mut Ui, state: &State, fill: Color32, name: &str, path: &str) {
    // Its size asked for first: a wrapping row can then tell when to wrap.
    ui.allocate_ui(vec2(88.0, 40.0), |ui| {
        ui.vertical(|ui| {
            swatch(ui, fill, vec2(88.0, 40.0)).on_hover_text(path);
            ui.label(name);
            if state.show_paths {
                ui.small(path);
            }
        });
    });
}

/// A row of swatches wrapping at the page's edge, their tops in line.
fn wrapped_row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let layout = egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true);
    ui.with_layout(layout, add);
}

pub fn show(ui: &mut Ui, state: &mut State) {
    let t = Tokens::of(ui.ctx());
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            section(ui, "Surfaces");
            wrapped_row(ui, |ui| {
                let s = &t.surface;
                for (fill, name, path) in [
                    (s.canvas, "canvas", "surface.canvas"),
                    (s.sunken, "sunken", "surface.sunken"),
                    (s.panel, "panel", "surface.panel"),
                    (s.overlay, "overlay", "surface.overlay"),
                    (s.raised, "raised", "surface.raised"),
                    (s.hover, "hover", "surface.hover"),
                    (s.active, "active", "surface.active"),
                ] {
                    named(ui, state, fill, name, path);
                }
            });

            section(ui, "Text");
            text_samples(ui, &t);

            section(ui, "Accent");
            wrapped_row(ui, |ui| {
                let a = &t.accent;
                for (fill, name, path) in [
                    (a.default, "default", "accent.default"),
                    (a.hover, "hover", "accent.hover"),
                    (a.active, "active", "accent.active"),
                    (a.subtle, "subtle", "accent.subtle"),
                    (t.selection, "selection", "selection"),
                    (t.focus_ring, "focus ring", "focus.ring"),
                ] {
                    named(ui, state, fill, name, path);
                }
            });

            section(ui, "Status");
            ui.horizontal_wrapped(|ui| {
                let s = &t.status;
                for (color, name) in [
                    (s.danger, "danger"),
                    (s.warning, "warning"),
                    (s.success, "success"),
                    (s.info, "info"),
                ] {
                    ui.colored_label(color, name);
                }
            });

            section(ui, "Spacing and shape");
            spacing(ui, &t);

            if state.show_contrast {
                section(ui, "Contrast");
                contrast_table(ui, &t);
            }
        });
}

fn text_samples(ui: &mut Ui, t: &Tokens) {
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(t.text.strong, "Strong text");
        ui.colored_label(t.text.default, "Default text");
        ui.colored_label(t.text.muted, "Muted text");
        // Text on the accent, painted as a primary button would be.
        let font = FontId::proportional(t.font.body);
        let galley =
            ui.painter()
                .layout_no_wrap("On the accent".into(), font.clone(), t.text.on_accent);
        let size = galley.size() + vec2(2.0 * t.space.m, 2.0 * t.space.s);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::same(
                Kit::of(ui.ctx())
                    .button
                    .style(kit::State::REST)
                    .radius
                    .round() as u8,
            ),
            t.accent.default,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "On the accent",
            font,
            t.text.on_accent,
        );
    });
    ui.horizontal(|ui| {
        ui.small("Small");
        ui.label("Body");
        ui.monospace("Monospace 0123456789");
    });
}

fn spacing(ui: &mut Ui, t: &Tokens) {
    egui::Grid::new("overview.spacing")
        .num_columns(3)
        .spacing(vec2(t.space.m, t.space.s))
        .show(ui, |ui| {
            for (name, value) in [
                ("space.xs", t.space.xs),
                ("space.s", t.space.s),
                ("space.m", t.space.m),
                ("space.l", t.space.l),
                ("space.xl", t.space.xl),
                ("size.control.height", t.size.control_height),
            ] {
                ui.monospace(name);
                ui.label(format!("{value}px"));
                let (rect, _) = ui.allocate_exact_size(vec2(value, 10.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, CornerRadius::ZERO, t.accent.default);
                ui.end_row();
            }
        });
    ui.add_space(t.space.m);
    ui.horizontal(|ui| {
        for (name, radius, stroke) in [
            (
                "radius.widget",
                t.radius.widget,
                Stroke::new(1.0, t.border.strong),
            ),
            (
                "radius.window",
                t.radius.window,
                Stroke::new(1.0, t.border.strong),
            ),
            (
                "focus ring",
                t.radius.widget,
                Stroke::new(2.0, t.focus_ring),
            ),
        ] {
            ui.vertical(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(88.0, 40.0), egui::Sense::hover());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(radius.round() as u8),
                    t.surface.raised,
                    stroke,
                    StrokeKind::Inside,
                );
                ui.small(name);
            });
        }
    });
}

fn contrast_table(ui: &mut Ui, t: &Tokens) {
    let r = t.resolved();
    let level = if r.spec().base.contains("high-contrast") {
        Level::High
    } else {
        Level::Normal
    };
    egui::Grid::new("overview.contrast")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Foreground on background");
            ui.strong("Ratio");
            ui.strong("Needs");
            ui.strong("");
            ui.end_row();
            for pair in contrast::PAIRS {
                let ratio = contrast::pair_ratio(r, pair);
                let needed = level.required(pair.kind);
                ui.monospace(format!("{} on {}", pair.foreground, pair.background));
                ui.label(ratio.map_or("-".into(), |r| format!("{r:.2}:1")));
                ui.label(format!("{needed}:1"));
                if ratio.is_some_and(|r| r >= needed) {
                    ui.colored_label(t.status.success, "pass");
                } else {
                    ui.colored_label(t.status.danger, "fail");
                }
                ui.end_row();
            }
        });
}
