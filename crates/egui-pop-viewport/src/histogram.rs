//! A histogram drawn from bins the application computed (the scopes'
//! computation stays with the application): one filled curve per
//! channel, linear or logarithmic, with clipping markers and a drag
//! handed back to the application.
//!
//! - **Channels**: each [`Channel`] has its bins (any scale; they are
//!   normalised together) and its colour.
//! - **Clipping**: [`Histogram::clipping`] lights a marker in a bottom
//!   corner when shadows or highlights are clipped (the fractions the
//!   application measured), in the `histogram.clip-*` colours.
//! - **Drag**: [`Histogram::draggable`] makes the histogram a control,
//!   as in darktable where dragging the histogram drags the exposure.
//!   The widget owns no value: it hands the gesture back as
//!   [`Change`]s of a [`Drag`] (where the gesture began, and how far it
//!   moved, in widths of the histogram). Ctrl (Cmd) is ten times finer,
//!   Shift ten times coarser; Escape gives the gesture up. When focused,
//!   the left and right arrows move by a step.
//!
//! The changes in [`HistogramOutput::changes`] say how far the gesture
//! has moved so far, from `from` to `to`: applying `to.delta -
//! from.delta` to the application's value each time
//! ([`HistogramOutput::moved`]) keeps it in step, including when a hook
//! vetoes or redirects the gesture at its end, or when Escape gives it
//! up (a change back to 0 comes then).
//!
//! The [`on_change`](Histogram::on_change) hook sees the drag's frames
//! (`done: false`), its end once more from 0 (`done: true`), and each
//! key; `egui-pop-imaging` runs the finished ones as the
//! `histogram.drag` step of an application.
//!
//! ```
//! use egui_pop_viewport::histogram::{Channel, Clipping, Histogram};
//!
//! fn ui(ui: &mut egui::Ui, luma: &[f32], exposure: &mut f64) {
//!     let out = Histogram::new([Channel::new("Luminance", luma, egui::Color32::GRAY)])
//!         .log(true)
//!         .clipping(Clipping { shadows: 0.0, highlights: 0.004 })
//!         .draggable(true)
//!         .show(ui);
//!     // Four stops per width of the histogram.
//!     *exposure += f64::from(out.moved()) * 4.0;
//! }
//! ```

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{
    Color32, CursorIcon, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, Widget,
    WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use serde_json::{Value as Json, json};

use crate::change::{self, Cause, Change, Gesture, Hook, StepValue, Verdict};

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

/// What is clipped, as the application measured it: the fraction of
/// pixels (0 to 1) at or below black, and at or above white.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clipping {
    pub shadows: f32,
    pub highlights: f32,
}

/// A drag on the histogram: where it began (a fraction of the width, 0
/// on the left; `None` for keys and assistive actions) and how far it
/// has moved, in widths of the histogram (positive to the right).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drag {
    pub at: Option<f32>,
    pub delta: f32,
}

/// `{"at": <0..1> | null, "delta": <widths>}`.
impl StepValue for Drag {
    fn to_json(&self) -> Json {
        json!({ "at": self.at, "delta": self.delta })
    }

    fn from_json(v: &Json) -> Option<Self> {
        let at = match v.get("at") {
            None | Some(Json::Null) => None,
            Some(a) => Some(change::json_f32(a)?),
        };
        Some(Drag {
            at,
            delta: change::json_f32(v.get("delta")?)?,
        })
    }
}

/// The histogram's colours, from the theme (`histogram.*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HistogramTokens {
    pub background: Color32,
    pub grid: Color32,
    pub clip_shadows: Color32,
    pub clip_highlights: Color32,
}

impl HistogramTokens {
    pub fn of(ctx: &egui::Context) -> HistogramTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        HistogramTokens {
            background: c("histogram.background", t.surface.sunken),
            grid: c("histogram.grid", t.border.default),
            clip_shadows: c("histogram.clip-shadows", t.status.info),
            clip_highlights: c("histogram.clip-highlights", t.status.danger),
        }
    }
}

/// The histogram widget.
#[derive(Debug)]
pub struct Histogram<'a> {
    channels: Vec<Channel<'a>>,
    log: bool,
    size: Option<Vec2>,
    label: Option<String>,
    clipping: Option<Clipping>,
    draggable: bool,
    step: f32,
    hook: Option<Hook<'a, Drag>>,
}

/// What the histogram did in a frame.
#[derive(Debug)]
pub struct HistogramOutput {
    /// The histogram; `changed()` when the drag moved.
    pub response: Response,
    /// The drag's changes written this frame, after the hook.
    pub changes: Vec<Change<Drag>>,
}

impl HistogramOutput {
    /// How far the drag moved this frame, in widths (the sum of
    /// `to.delta - from.delta`): add it, scaled, to the value the drag
    /// adjusts.
    pub fn moved(&self) -> f32 {
        self.changes.iter().map(|c| c.to.delta - c.from.delta).sum()
    }

    /// Where the gesture moving this frame began (`None` for keys).
    pub fn at(&self) -> Option<f32> {
        self.changes.iter().find_map(|c| c.to.at)
    }
}

impl<'a> Histogram<'a> {
    pub fn new(channels: impl IntoIterator<Item = Channel<'a>>) -> Histogram<'a> {
        Histogram {
            channels: channels.into_iter().collect(),
            log: false,
            size: None,
            label: None,
            clipping: None,
            draggable: false,
            step: 0.01,
            hook: None,
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

    /// Lights the clipping markers: shadows in the bottom-left corner,
    /// highlights in the bottom-right one, when their fraction is above
    /// zero.
    pub fn clipping(mut self, clipping: Clipping) -> Histogram<'a> {
        self.clipping = Some(clipping);
        self
    }

    /// Makes the histogram a control whose drag is handed back to the
    /// application ([`HistogramOutput::changes`]).
    pub fn draggable(mut self, on: bool) -> Histogram<'a> {
        self.draggable = on;
        self
    }

    /// How far an arrow key moves, in widths (a hundredth by default;
    /// Ctrl finer, Shift coarser).
    pub fn step(mut self, step: f32) -> Histogram<'a> {
        self.step = step.abs();
        self
    }

    /// Sees each change of the drag before it is written: allow, veto
    /// (at the end: the gesture is given back) or redirect (another
    /// distance). Makes the histogram draggable.
    pub fn on_change(
        mut self,
        hook: impl FnMut(&Change<Drag>) -> Verdict<Drag> + 'a,
    ) -> Histogram<'a> {
        self.hook = Some(Hook::new(hook));
        self.draggable = true;
        self
    }

    /// Draws the histogram; says how the drag moved.
    pub fn show(self, ui: &mut Ui) -> HistogramOutput {
        let Histogram {
            channels,
            log,
            size,
            label,
            clipping,
            draggable,
            step,
            mut hook,
        } = self;
        let size = size.unwrap_or_else(|| {
            let w = ui.available_width().clamp(120.0, 600.0);
            vec2(w, (w / 3.0).round())
        });
        let sense = if draggable {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);
        let mut changes = Vec::new();
        if draggable {
            drag(ui, &response, rect, step, &mut hook, &mut changes);
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }
            if !changes.is_empty() {
                response.mark_changed();
            }
        }
        paint(ui, rect, &response, &channels, log, clipping, draggable);
        let names: Vec<&str> = channels.iter().map(|c| c.name).collect();
        let bins = channels.first().map_or(0, |c| c.bins.len());
        let label = label.unwrap_or_else(|| strings::tr(ui.ctx(), &crate::strings::HISTOGRAM));
        let value = value_text(ui.ctx(), &names, bins, log, clipping);
        let kind = if draggable {
            WidgetType::Slider
        } else {
            WidgetType::Other
        };
        let enabled = ui.is_enabled();
        response.widget_info(move || {
            let mut info = WidgetInfo::labeled(kind, enabled, &label);
            info.current_text_value = Some(value.clone());
            info
        });
        if draggable {
            ui.ctx().accesskit_node_builder(response.id, |node| {
                use egui::accesskit::Action;
                node.add_action(Action::Increment);
                node.add_action(Action::Decrement);
            });
        }
        HistogramOutput { response, changes }
    }
}

impl Widget for Histogram<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

fn change_of(from: Drag, to: Drag, cause: Cause, done: bool) -> Change<Drag> {
    Change {
        from,
        to,
        cause,
        done,
    }
}

/// The drag, the keys and assistive actions, written to `changes`.
fn drag(
    ui: &mut Ui,
    response: &Response,
    rect: Rect,
    step: f32,
    hook: &mut Option<Hook<'_, Drag>>,
    changes: &mut Vec<Change<Drag>>,
) {
    let id = response.id.with("egui_pop_histogram_drag");
    if response.drag_started() {
        let at = response
            .interact_pointer_pos()
            .map(|p| ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0));
        ui.data_mut(|d| d.insert_temp(id, Drag { at, delta: 0.0 }));
    }
    let current = ui.data(|d| d.get_temp::<Drag>(id));
    match change::gesture(ui, response) {
        Gesture::None => {}
        Gesture::Moved(d) => {
            if let Some(from) = current {
                let to = Drag {
                    delta: from.delta + d.x / rect.width().max(1.0),
                    ..from
                };
                if let Some(c) = change::decide(hook, change_of(from, to, Cause::Drag, false)) {
                    ui.data_mut(|data| data.insert_temp(id, c.to));
                    changes.push(c);
                }
            }
        }
        Gesture::Ended => {
            ui.data_mut(|d| d.remove::<Drag>(id));
            if let Some(total) = current.filter(|t| t.delta != 0.0) {
                let start = Drag {
                    delta: 0.0,
                    ..total
                };
                // The hook sees the whole gesture; the output says how
                // far it moves from where it is to where it ends.
                let end = change::decide(hook, change_of(start, total, Cause::Drag, true))
                    .map_or(start, |c| c.to);
                changes.push(change_of(total, end, Cause::Drag, true));
            }
        }
        Gesture::Cancelled => {
            ui.data_mut(|d| d.remove::<Drag>(id));
            if let Some(total) = current.filter(|t| t.delta != 0.0) {
                let start = Drag {
                    delta: 0.0,
                    ..total
                };
                changes.push(change_of(total, start, Cause::Drag, true));
            }
        }
    }
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    let mut keyed = |steps: f32, cause: Cause| {
        if steps == 0.0 {
            return;
        }
        let to = Drag {
            at: None,
            delta: steps * step,
        };
        if let Some(c) = change::decide(hook, change_of(Drag::default(), to, cause, true)) {
            changes.push(c);
        }
    };
    if response.has_focus() {
        change::lock_arrows(ui, response.id);
        let [x, _] = change::arrows(ui);
        keyed(x * change::scale(ui), Cause::Key);
    }
    keyed(change::access_steps(ui, response.id), Cause::Access);
}

/// The background, the quarter lines, the curves and the markers.
fn paint(
    ui: &Ui,
    rect: Rect,
    response: &Response,
    channels: &[Channel<'_>],
    log: bool,
    clipping: Option<Clipping>,
    draggable: bool,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let t = HistogramTokens::of(ui.ctx());
    let tokens = Tokens::of(ui.ctx());
    let radius = tokens.radius.widget;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, radius, t.background);
    let hair = 1.0 / ui.ctx().pixels_per_point();
    for q in 1..4 {
        let x = rect.left() + rect.width() * q as f32 / 4.0;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(hair, t.grid),
        );
    }
    let max = channels
        .iter()
        .flat_map(|c| c.bins.iter().copied())
        .fold(0.0f32, f32::max);
    for c in channels {
        let h = heights(c.bins, max, log);
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
    if let Some(clip) = clipping {
        let side = (rect.height() * 0.12).clamp(6.0, 12.0);
        let inset = 2.0;
        let marker = |corner: Pos2, inward: f32, color: Color32, lit: bool| {
            let points = vec![
                corner,
                corner + vec2(0.0, -side),
                corner + vec2(inward * side, 0.0),
            ];
            if lit {
                painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
            } else {
                painter.add(Shape::closed_line(points, Stroke::new(hair, t.grid)));
            }
        };
        marker(
            pos2(rect.left() + inset, rect.bottom() - inset),
            1.0,
            t.clip_shadows,
            clip.shadows > 0.0,
        );
        marker(
            pos2(rect.right() - inset, rect.bottom() - inset),
            -1.0,
            t.clip_highlights,
            clip.highlights > 0.0,
        );
    }
    if draggable {
        if response.has_focus() {
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(2.0, tokens.focus_ring),
                StrokeKind::Inside,
            );
        } else if response.hovered() {
            let visuals = ui.style().interact(response);
            painter.rect_stroke(rect, radius, visuals.bg_stroke, StrokeKind::Inside);
        }
    }
}

/// What the histogram shows, for screen readers.
fn value_text(
    ctx: &egui::Context,
    names: &[&str],
    bins: usize,
    log: bool,
    clipping: Option<Clipping>,
) -> String {
    let mut value = strings::tr_args(
        ctx,
        if log {
            &crate::strings::HISTOGRAM_VALUE_LOG
        } else {
            &crate::strings::HISTOGRAM_VALUE
        },
        &[("channels", &names.join(", ")), ("bins", &bins.to_string())],
    );
    if let Some(clip) = clipping {
        for (fraction, key) in [
            (clip.shadows, &crate::strings::HISTOGRAM_CLIPPED_SHADOWS),
            (
                clip.highlights,
                &crate::strings::HISTOGRAM_CLIPPED_HIGHLIGHTS,
            ),
        ] {
            if fraction > 0.0 {
                let percent = strings::number(ctx, f64::from(fraction) * 100.0, 1);
                value.push_str(", ");
                value.push_str(&strings::tr_args(ctx, key, &[("percent", &percent)]));
            }
        }
    }
    value
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

    #[test]
    fn a_drag_round_trips_through_json() {
        for d in [
            Drag {
                at: Some(0.75),
                delta: -0.25,
            },
            Drag {
                at: None,
                delta: 0.01,
            },
        ] {
            assert_eq!(Drag::from_json(&d.to_json()), Some(d));
        }
    }
}
