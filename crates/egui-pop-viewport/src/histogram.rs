//! A histogram drawn from bins the application computed (the scopes'
//! computation stays with the application): one filled curve per
//! channel, linear or logarithmic.
//!
//! ```
//! use egui_pop_viewport::histogram::{Channel, Histogram};
//!
//! fn ui(ui: &mut egui::Ui, luma: &[f32]) {
//!     ui.add(Histogram::new([Channel::new("Luminance", luma, egui::Color32::GRAY)]).log(true));
//! }
//! ```

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{
    Color32, Pos2, Response, Sense, Shape, Stroke, Ui, Vec2, Widget, WidgetInfo, WidgetType, vec2,
};
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;

/// One channel: its bins (any scale; they are normalised) and colour.
#[derive(Clone, Debug)]
pub struct Channel<'a> {
    pub name: &'a str,
    pub bins: &'a [f32],
    pub color: Color32,
}

impl<'a> Channel<'a> {
    pub fn new(name: &'a str, bins: &'a [f32], color: Color32) -> Channel<'a> {
        Channel { name, bins, color }
    }
}

/// The histogram's colours, from the theme (`histogram.*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HistogramTokens {
    pub background: Color32,
    pub grid: Color32,
}

impl HistogramTokens {
    pub fn of(ctx: &egui::Context) -> HistogramTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        HistogramTokens {
            background: c("histogram.background", t.surface.sunken),
            grid: c("histogram.grid", t.border.default),
        }
    }
}

/// The histogram widget.
#[derive(Clone, Debug)]
pub struct Histogram<'a> {
    channels: Vec<Channel<'a>>,
    log: bool,
    size: Option<Vec2>,
    label: Option<String>,
}

impl<'a> Histogram<'a> {
    pub fn new(channels: impl IntoIterator<Item = Channel<'a>>) -> Histogram<'a> {
        Histogram {
            channels: channels.into_iter().collect(),
            log: false,
            size: None,
            label: None,
        }
    }

    /// Heights on a logarithmic scale (small counts stay visible).
    pub fn log(mut self, log: bool) -> Histogram<'a> {
        self.log = log;
        self
    }

    /// The size in points (default: the available width, a third as
    /// high).
    pub fn size(mut self, size: Vec2) -> Histogram<'a> {
        self.size = Some(size);
        self
    }

    /// The accessible name (default "Histogram").
    pub fn label(mut self, label: impl Into<String>) -> Histogram<'a> {
        self.label = Some(label.into());
        self
    }
}

/// The heights (0..1) of a channel's bins, normalised by `max`.
pub fn heights(bins: &[f32], max: f32, log: bool) -> Vec<f32> {
    if max <= 0.0 {
        return vec![0.0; bins.len()];
    }
    bins.iter()
        .map(|b| {
            let b = b.max(0.0);
            if log {
                (1.0 + b).ln() / (1.0 + max).ln()
            } else {
                b / max
            }
        })
        .collect()
}

impl Widget for Histogram<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = self.size.unwrap_or_else(|| {
            let w = ui.available_width().clamp(120.0, 600.0);
            vec2(w, (w / 3.0).round())
        });
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let t = HistogramTokens::of(ui.ctx());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, Tokens::of(ui.ctx()).radius.widget, t.background);
        for q in 1..4 {
            let x = rect.left() + rect.width() * q as f32 / 4.0;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0 / ui.ctx().pixels_per_point(), t.grid),
            );
        }
        let max = self
            .channels
            .iter()
            .flat_map(|c| c.bins.iter().copied())
            .fold(0.0f32, f32::max);
        for c in &self.channels {
            let h = heights(c.bins, max, self.log);
            if h.is_empty() {
                continue;
            }
            let n = h.len() as f32;
            let at = |i: usize, v: f32| {
                Pos2::new(
                    rect.left() + rect.width() * (i as f32 + 0.5) / n,
                    rect.bottom() - rect.height() * v,
                )
            };
            // The area under the curve: one quad per step, as a mesh
            // (the outline is not convex).
            let fill = c.color.gamma_multiply(0.35);
            let mut mesh = Mesh::default();
            for (i, v) in h.iter().enumerate() {
                let top = at(i, *v);
                let base = mesh.vertices.len() as u32;
                mesh.vertices.push(Vertex {
                    pos: top,
                    uv: WHITE_UV,
                    color: fill,
                });
                mesh.vertices.push(Vertex {
                    pos: Pos2::new(top.x, rect.bottom()),
                    uv: WHITE_UV,
                    color: fill,
                });
                if i > 0 {
                    mesh.add_triangle(base - 2, base - 1, base);
                    mesh.add_triangle(base - 1, base + 1, base);
                }
            }
            painter.add(Shape::mesh(mesh));
            let line: Vec<Pos2> = h.iter().enumerate().map(|(i, v)| at(i, *v)).collect();
            painter.add(Shape::line(line, Stroke::new(1.0, c.color)));
        }
        let names: Vec<&str> = self.channels.iter().map(|c| c.name).collect();
        let bins = self.channels.first().map_or(0, |c| c.bins.len());
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| strings::tr(ui.ctx(), &crate::strings::HISTOGRAM));
        let value = strings::tr_args(
            ui.ctx(),
            if self.log {
                &crate::strings::HISTOGRAM_VALUE_LOG
            } else {
                &crate::strings::HISTOGRAM_VALUE
            },
            &[("channels", &names.join(", ")), ("bins", &bins.to_string())],
        );
        response.widget_info(move || {
            let mut info = WidgetInfo::labeled(WidgetType::Other, true, &label);
            info.current_text_value = Some(value.clone());
            info
        });
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_are_normalised_linear_or_log() {
        assert_eq!(heights(&[0.0, 5.0, 10.0], 10.0, false), vec![0.0, 0.5, 1.0]);
        let l = heights(&[0.0, 1.0, 100.0], 100.0, true);
        assert_eq!(l[0], 0.0);
        assert!((l[2] - 1.0).abs() < 1e-6);
        assert!(l[1] > 0.1, "small counts stay visible: {}", l[1]);
        assert_eq!(heights(&[1.0], 0.0, false), vec![0.0]);
    }
}
