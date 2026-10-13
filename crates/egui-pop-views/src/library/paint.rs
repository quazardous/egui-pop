//! The library's colours (from the theme) and the shapes its marks are
//! drawn with: stars, flags, colour labels (each a shape of its own, so
//! a label never reads by its colour alone), pills.

use std::f32::consts::{PI, TAU};

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use egui_pop_core::theme::Tokens;

use super::marks::ColorLabel;

/// The library's colours, from the installed theme: optional component
/// tokens (`library.*`) falling back on system ones, so every theme
/// draws them without declaring anything.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryTokens {
    /// Behind a cell's image (`thumbnail.background`).
    pub cell: Color32,
    /// A cell under the pointer (`thumbnail.hover`).
    pub hover: Color32,
    /// A selected cell's outline (`thumbnail.selected`).
    pub selected: Color32,
    /// A selected cell's background (`library.selected-background`).
    pub selected_background: Color32,
    /// A star given (`library.star`, else `status.warning`).
    pub star: Color32,
    /// A star not given, shown under the pointer (`library.star-empty`).
    pub star_empty: Color32,
    /// The pick flag (`library.pick`, else `text.strong`).
    pub pick: Color32,
    /// The reject mark (`library.reject`, else `status.danger`).
    pub reject: Color32,
    /// Red, yellow, green, blue and purple (`library.label-<name>`).
    pub labels: [Color32; 5],
    /// The pills overlays sit on (`library.overlay`).
    pub overlay: Color32,
    /// Text on a pill (`library.overlay-text`).
    pub overlay_text: Color32,
    /// Over a rejected image (`library.dim`).
    pub dim: Color32,
    /// The rubber band (`library.band`) and its outline.
    pub band: Color32,
    pub band_outline: Color32,
    /// The bar beside an expanded stack's members (`library.stack`).
    pub stack: Color32,
    /// The best image's outline and marker (`library.best`).
    pub best: Color32,
    /// Badge tones: neutral, info, success, warning, danger.
    pub tones: [Color32; 5],
    /// A chip's fill (`library.chip`) and its text.
    pub chip: Color32,
    pub chip_text: Color32,
}

impl LibraryTokens {
    pub fn of(ctx: &egui::Context) -> LibraryTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        let purple = mix(t.status.danger, t.status.info);
        LibraryTokens {
            cell: c("thumbnail.background", t.surface.raised),
            hover: c("thumbnail.hover", t.border.strong),
            selected: c("thumbnail.selected", t.accent.default),
            selected_background: c("library.selected-background", t.accent.subtle),
            star: c("library.star", t.status.warning),
            star_empty: c("library.star-empty", t.text.muted),
            pick: c("library.pick", t.text.strong),
            reject: c("library.reject", t.status.danger),
            labels: [
                c("library.label-red", t.status.danger),
                c("library.label-yellow", t.status.warning),
                c("library.label-green", t.status.success),
                c("library.label-blue", t.status.info),
                c("library.label-purple", purple),
            ],
            overlay: c("library.overlay", t.surface.overlay.gamma_multiply(0.85)),
            overlay_text: c("library.overlay-text", t.text.strong),
            dim: c("library.dim", t.surface.canvas.gamma_multiply(0.6)),
            band: c("library.band", t.accent.default.gamma_multiply(0.18)),
            band_outline: c("library.band-outline", t.accent.default),
            stack: c("library.stack", t.accent.subtle),
            best: c("library.best", t.status.success),
            tones: [
                t.surface.overlay,
                t.status.info,
                t.status.success,
                t.status.warning,
                t.status.danger,
            ],
            chip: c("library.chip", t.surface.active),
            chip_text: c("library.chip-text", t.text.default),
        }
    }

    /// A label's colour.
    pub fn label(&self, label: ColorLabel) -> Color32 {
        self.labels[label.index()]
    }
}

/// Halfway between two colours.
fn mix(a: Color32, b: Color32) -> Color32 {
    let m = |x: u8, y: u8| ((u16::from(x) + u16::from(y)) / 2) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// The ten points of a five-pointed star, outer first, from the top.
fn star_points(center: Pos2, outer: f32) -> Vec<Pos2> {
    let inner = outer * 0.42;
    (0..10usize)
        .map(|k| {
            let a = -PI / 2.0 + k as f32 * PI / 5.0;
            let r = if k.is_multiple_of(2) { outer } else { inner };
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

/// A star, filled (its pentagon and its five tips, each convex) or
/// outlined.
pub fn star(painter: &Painter, center: Pos2, radius: f32, fill: Option<Color32>, outline: Color32) {
    let p = star_points(center, radius);
    match fill {
        Some(color) => {
            let inner: Vec<Pos2> = p.iter().skip(1).step_by(2).copied().collect();
            painter.add(Shape::convex_polygon(inner, color, Stroke::NONE));
            for k in (0..10).step_by(2) {
                let tip = vec![p[(k + 9) % 10], p[k], p[k + 1]];
                painter.add(Shape::convex_polygon(tip, color, Stroke::NONE));
            }
        }
        None => {
            painter.add(Shape::closed_line(p, Stroke::new(1.0, outline)));
        }
    }
}

/// A pennant on its pole, in `rect`.
pub fn flag(painter: &Painter, rect: Rect, color: Color32) {
    let x0 = rect.left() + rect.width() * 0.25;
    let x1 = rect.right() - rect.width() * 0.1;
    let top = rect.top() + rect.height() * 0.1;
    let bottom = rect.bottom() - rect.height() * 0.05;
    painter.line_segment([pos2(x0, top), pos2(x0, bottom)], Stroke::new(1.5, color));
    painter.add(Shape::convex_polygon(
        vec![
            pos2(x0, top),
            pos2(x1, top + rect.height() * 0.28),
            pos2(x0, top + rect.height() * 0.56),
        ],
        color,
        Stroke::NONE,
    ));
}

/// A cross, in `rect`.
pub fn cross(painter: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.2);
    let stroke = Stroke::new(2.0, color);
    painter.line_segment([r.left_top(), r.right_bottom()], stroke);
    painter.line_segment([r.right_top(), r.left_bottom()], stroke);
}

/// A colour label's shape: red a circle, yellow a triangle, green a
/// square, blue a diamond, purple a hexagon.
pub fn label(painter: &Painter, label: ColorLabel, center: Pos2, radius: f32, color: Color32) {
    let poly = |n: usize, start: f32, r: f32| -> Vec<Pos2> {
        (0..n)
            .map(|k| {
                let a = start + k as f32 * TAU / n as f32;
                center + vec2(a.cos(), a.sin()) * r
            })
            .collect()
    };
    let shape = match label {
        ColorLabel::Red => Shape::circle_filled(center, radius * 0.9, color),
        ColorLabel::Yellow => {
            Shape::convex_polygon(poly(3, -PI / 2.0, radius * 1.1), color, Stroke::NONE)
        }
        ColorLabel::Green => Shape::rect_filled(
            Rect::from_center_size(center, vec2(radius, radius) * 1.6),
            CornerRadius::ZERO,
            color,
        ),
        ColorLabel::Blue => {
            Shape::convex_polygon(poly(4, -PI / 2.0, radius * 1.05), color, Stroke::NONE)
        }
        ColorLabel::Purple => Shape::convex_polygon(poly(6, 0.0, radius), color, Stroke::NONE),
    };
    painter.add(shape);
}

/// A pill holding `text`, its right edge or left edge at `at`; returns
/// its rectangle.
pub fn pill(
    painter: &Painter,
    at: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    fill: Color32,
    color: Color32,
) -> Rect {
    let galley = painter.layout_no_wrap(text.to_string(), font, color);
    let pad = vec2(4.0, 1.0);
    let size = galley.size() + pad * 2.0;
    let rect = anchor.anchor_size(at, size);
    painter.rect_filled(rect, CornerRadius::same((size.y / 2.0) as u8), fill);
    painter.galley(rect.min + pad, galley, color);
    rect
}
