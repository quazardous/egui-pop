//! Progress and waiting: a bar (determinate or indeterminate, with tones
//! and stacked sections), a ring, a meter and a spinner with a label and
//! a delay, drawn by the kit (`progress-bar`, `progress-line`,
//! `progress-fill`, `progress-caption`, `meter`, `meter-tick`,
//! `progress-ring`, `spinner`; docs/kit.md) on the motion clock
//! (`motion`): frozen in tests, still under reduced motion.
//!
//! Each is an AccessKit `ProgressIndicator` (the meter a `Meter`) with
//! a name, and, when determinate, a numeric value and its range (0 to
//! the total, or 0 to 100) and a value text ("12 of 340"); an
//! indeterminate one has no value.
//!
//! ```
//! use egui_pop_core::progress::{ProgressBar, Ring, Spinner, Tone};
//!
//! fn ui(ui: &mut egui::Ui) {
//!     ui.add(ProgressBar::new(Some(0.42)).label("Export"));
//!     ui.add(
//!         ProgressBar::counts(340, 1204)
//!             .label("Import")
//!             .section(0.05, Tone::Danger, "failed")
//!             .text("340 of 1,204, 61 failed"),
//!     );
//!     ui.add(ProgressBar::new(None).label("Scanning"));
//!     ui.add(Ring::new(Some(0.75)).label("Render"));
//!     ui.add(Spinner::new().label("Loading previews"));
//! }
//! ```

use std::f32::consts::TAU;

use egui::accesskit::Role;
use egui::{
    Color32, Context, FontId, Id, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, Widget,
    WidgetInfo, pos2, vec2,
};

pub use crate::kit::Tone;
use crate::kit::{Kit, KitCx, Part, State};
use crate::motion::{self, Busy};
use crate::theme::Tokens;

impl Tone {
    /// Its colour as a progress fill (`progress-fill`, in this tone).
    pub fn color(self, ctx: &Context) -> Color32 {
        Kit::of(ctx)
            .progress_fill
            .style_toned(self, State::REST)
            .fill
    }
}

/// The installed kit's progress values, for a widget drawing progress of
/// its own (read from `progress-bar`, `progress-fill`, `progress-line`
/// and `progress-ring`).
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressTokens {
    /// The track under the fill (`progress-bar.rest.fill`).
    pub track: Color32,
    /// The fill (`progress-fill.rest.fill`).
    pub fill: Color32,
    /// A bar's thickness (`progress-bar.height`).
    pub height: f32,
    /// A thin line's (`progress-line.height`).
    pub line: f32,
    /// A ring's and a spinner's diameter (`progress-ring.width`).
    pub ring: f32,
}

impl ProgressTokens {
    pub fn of(ctx: &Context) -> ProgressTokens {
        let kit = Kit::of(ctx);
        ProgressTokens {
            track: kit.progress_bar.style(State::REST).fill,
            fill: kit.progress_fill.style(State::REST).fill,
            height: kit.progress_bar.data.height.max(1.0),
            line: kit.progress_line.data.height.max(1.0),
            ring: kit.progress_ring.data.width.max(8.0),
        }
    }
}

/// A stacked section of a bar: done, failed, skipped.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// Its share of the whole bar (0 to 1).
    pub fraction: f32,
    pub tone: Tone,
    pub label: String,
}

/// How thick a bar is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarSize {
    /// A bar (`progress.height`).
    #[default]
    Standard,
    /// A thin line under a title or along a viewport's top
    /// (`progress.line`).
    Line,
}

/// The value AccessKit gets: a number within a range, as text.
#[derive(Clone, Debug, PartialEq)]
struct Value {
    value: f64,
    min: f64,
    max: f64,
}

fn percent(fraction: f32) -> String {
    format!("{:.0}%", f64::from(fraction.clamp(0.0, 1.0)) * 100.0)
}

/// Writes a progress node: its name, and its value and range when
/// determinate (none when indeterminate).
fn access(
    ui: &Ui,
    response: &Response,
    role: Option<Role>,
    label: &str,
    value: Option<&Value>,
    text: Option<&str>,
) {
    let enabled = ui.is_enabled();
    let name = label.to_string();
    response.widget_info(|| WidgetInfo::labeled(Role::ProgressIndicator, enabled, &name));
    let (value, text) = (value.cloned(), text.map(String::from));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        if let Some(role) = role {
            node.set_role(role);
        }
        match &value {
            Some(v) => {
                node.set_numeric_value(v.value);
                node.set_min_numeric_value(v.min);
                node.set_max_numeric_value(v.max);
            }
            None => {
                node.clear_numeric_value();
                node.clear_min_numeric_value();
                node.clear_max_numeric_value();
            }
        }
        match text {
            Some(t) => node.set_value(t),
            None => node.clear_value(),
        }
    });
}

/// Paints a stretch of a track (`progress-fill`) in a tone: `room` is
/// the track, the stretch's ends rounded where they meet its ends.
fn paint_fill(kit: &Kit, cx: &KitCx<'_>, rect: Rect, room: Rect, tone: Tone) {
    kit.progress_fill.paint(
        cx,
        &Part {
            rect,
            room,
            state: State::REST.toned(tone),
            label: None,
        },
    );
}

/// A progress bar.
#[derive(Clone, Debug)]
#[must_use = "add it with `ui.add`"]
pub struct ProgressBar {
    fraction: Option<f32>,
    sections: Vec<Section>,
    tone: Tone,
    size: BarSize,
    label: String,
    text: Option<String>,
    counts: Option<(u64, u64)>,
    width: Option<f32>,
    caption: bool,
}

impl ProgressBar {
    /// A bar at `fraction` (0 to 1); none: indeterminate (a segment
    /// travelling along the track).
    pub fn new(fraction: Option<f32>) -> Self {
        Self {
            fraction: fraction.map(|f| f.clamp(0.0, 1.0)),
            sections: Vec::new(),
            tone: Tone::Accent,
            size: BarSize::Standard,
            label: String::new(),
            text: None,
            counts: None,
            width: None,
            caption: false,
        }
    }

    /// An indeterminate bar.
    pub fn indeterminate() -> Self {
        Self::new(None)
    }

    /// `done` of `total` items: AccessKit's range is 0 to `total`, the
    /// value text "done of total".
    pub fn counts(done: u64, total: u64) -> Self {
        let fraction = if total == 0 {
            0.0
        } else {
            (done as f64 / total as f64) as f32
        };
        let mut bar = Self::new(Some(fraction));
        bar.counts = Some((done, total));
        bar
    }

    /// Its accessible name ("Exporting 340 images").
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Its value as text ("12 of 340, about 2 minutes left"), for
    /// screen readers and, with [`ProgressBar::caption`], on screen.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Adds a stacked section after the fill (failed, skipped): the
    /// fill keeps its fraction, the sections follow it.
    pub fn section(mut self, fraction: f32, tone: Tone, label: impl Into<String>) -> Self {
        self.sections.push(Section {
            fraction: fraction.clamp(0.0, 1.0),
            tone,
            label: label.into(),
        });
        self
    }

    /// The thin line.
    pub fn line(mut self) -> Self {
        self.size = BarSize::Line;
        self
    }

    pub fn size(mut self, size: BarSize) -> Self {
        self.size = size;
        self
    }

    /// Its width (the available width by default).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Draws the name and the value text above the bar.
    pub fn caption(mut self, on: bool) -> Self {
        self.caption = on;
        self
    }

    fn value(&self) -> Option<Value> {
        match (self.counts, self.fraction) {
            (Some((done, total)), _) => Some(Value {
                value: done as f64,
                min: 0.0,
                max: total as f64,
            }),
            (None, Some(f)) => Some(Value {
                value: (f64::from(f) * 100.0).round(),
                min: 0.0,
                max: 100.0,
            }),
            (None, None) => None,
        }
    }

    fn value_text(&self) -> Option<String> {
        self.text
            .clone()
            .or_else(|| match (self.counts, self.fraction) {
                (Some((done, total)), _) => Some(format!("{done} of {total}")),
                (None, Some(f)) => Some(percent(f)),
                (None, None) => None,
            })
    }
}

impl Widget for ProgressBar {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let kit = Kit::of(&ctx);
        let track_kit = match self.size {
            BarSize::Standard => &kit.progress_bar,
            BarSize::Line => &kit.progress_line,
        };
        let thick = track_kit.data.height.max(1.0);
        let width = self
            .width
            .unwrap_or_else(|| ui.available_width())
            .max(thick * 4.0);
        let font = FontId::proportional(t.font.small);
        let caption_h = if self.caption {
            ui.ctx().fonts_mut(|f| f.row_height(&font)) + t.space.xs
        } else {
            0.0
        };
        let (rect, response) =
            ui.allocate_exact_size(vec2(width, caption_h + thick), Sense::hover());
        let value_text = self.value_text();
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let cx = KitCx {
                painter,
                ppp: ctx.pixels_per_point(),
            };
            if self.caption {
                // The name, then the value text in the muted tone, at the
                // row's end.
                let caption = &kit.progress_caption;
                let text = |s: &str, end: bool, tone: Tone| {
                    let g =
                        painter.layout_no_wrap(s.to_string(), font.clone(), Color32::PLACEHOLDER);
                    let at = if end {
                        pos2(rect.right() - g.size().x, rect.top())
                    } else {
                        rect.left_top()
                    };
                    let r = Rect::from_min_size(at, g.size());
                    caption.paint_part(
                        &cx,
                        &Part {
                            rect: r,
                            room: r,
                            state: State::REST.toned(tone),
                            label: Some(&g),
                        },
                    );
                };
                text(&self.label, false, Tone::Neutral);
                if let Some(v) = &value_text {
                    text(v, true, Tone::Muted);
                }
            }
            let track = Rect::from_min_max(pos2(rect.left(), rect.bottom() - thick), rect.max);
            track_kit.paint_part(
                &cx,
                &Part {
                    rect: track,
                    room: track,
                    state: State::REST,
                    label: None,
                },
            );
            match self.fraction {
                Some(f) => {
                    let parts = std::iter::once((f, self.tone))
                        .chain(self.sections.iter().map(|s| (s.fraction, s.tone)));
                    let mut x = track.left();
                    for (share, tone) in parts {
                        let w = (share * track.width()).min(track.right() - x);
                        if w <= 0.0 {
                            continue;
                        }
                        let part =
                            Rect::from_min_max(pos2(x, track.top()), pos2(x + w, track.bottom()));
                        paint_fill(&kit, &cx, part, track, tone);
                        x += w;
                    }
                }
                None if motion::reduced(&ctx) => {
                    // Still: a striped track, the fill's colour at 60 %.
                    let tone = kit.progress_fill.style_toned(self.tone, State::REST).fill;
                    let step = (thick * 2.0).max(6.0);
                    let clip = painter.with_clip_rect(track);
                    let mut x = track.left();
                    while x < track.right() {
                        let part = Rect::from_min_max(
                            pos2(x, track.top()),
                            pos2((x + step / 2.0).min(track.right()), track.bottom()),
                        );
                        clip.rect_filled(part, 0.0, tone.gamma_multiply(0.6));
                        x += step;
                    }
                }
                None => {
                    let seg = track.width() * 0.3;
                    let phase = motion::phase(&ctx, f64::from(t.motion.progress_travel), 0.0);
                    let x = track.left() - seg + phase * (track.width() + seg);
                    let part =
                        Rect::from_min_max(pos2(x, track.top()), pos2(x + seg, track.bottom()));
                    // Its own room: rounded at both ends, clipped by the
                    // track.
                    let clip = painter.with_clip_rect(track);
                    let clipped = KitCx {
                        painter: &clip,
                        ppp: cx.ppp,
                    };
                    paint_fill(&kit, &clipped, part, part, self.tone);
                    motion::animate(&ctx);
                }
            }
        }
        let response = if self.label.is_empty() {
            response
        } else {
            response.on_hover_text(match &value_text {
                Some(v) => format!("{}: {v}", self.label),
                None => self.label.clone(),
            })
        };
        access(
            ui,
            &response,
            None,
            &self.label,
            self.value().as_ref(),
            value_text.as_deref(),
        );
        response
    }
}

/// Points along an arc of a circle, from `start` turning `sweep`
/// radians clockwise (screen y down).
fn arc(center: Pos2, radius: f32, start: f32, sweep: f32) -> Vec<Pos2> {
    let n = ((sweep.abs() / TAU) * 48.0).ceil().max(2.0) as usize;
    (0..=n)
        .map(|i| {
            let a = start + sweep * i as f32 / n as f32;
            center + radius * vec2(a.cos(), a.sin())
        })
        .collect()
}

/// Paints a ring in `rect`: an arc from the top for `fraction`, a
/// turning arc when none (still under reduced motion), the arc in
/// `color` over `progress-ring`'s track. For a ring that is part of
/// another widget; [`Ring`] is the widget.
pub fn paint_ring(ui: &Ui, rect: Rect, fraction: Option<f32>, color: Color32) {
    paint_ring_on(ui.painter(), rect, fraction, color);
}

/// [`paint_ring`] with a painter of its own (another layer, a clip).
pub fn paint_ring_on(painter: &Painter, rect: Rect, fraction: Option<f32>, color: Color32) {
    let ctx = painter.ctx();
    let t = Tokens::of(ctx);
    let track = Kit::of(ctx).progress_ring.style(State::REST).fill;
    let side = rect.width().min(rect.height());
    let width = (side / 7.0).max(1.5);
    let radius = side / 2.0 - width / 2.0;
    painter.circle_stroke(rect.center(), radius, Stroke::new(width, track));
    let top = -TAU / 4.0;
    match fraction {
        Some(f) if f > 0.0 => {
            painter.line(
                arc(rect.center(), radius, top, f.min(1.0) * TAU),
                Stroke::new(width, color),
            );
        }
        Some(_) => {}
        None => {
            let phase = motion::phase(ctx, f64::from(t.motion.spinner_turn), 0.125);
            painter.line(
                arc(rect.center(), radius, top + phase * TAU, TAU / 4.0),
                Stroke::new(width, color),
            );
            motion::animate(ctx);
        }
    }
}

/// A small circular progress: determinate (an arc from the top) or
/// indeterminate (a turning arc).
#[derive(Clone, Debug)]
#[must_use = "add it with `ui.add`"]
pub struct Ring {
    fraction: Option<f32>,
    tone: Tone,
    size: Option<f32>,
    label: String,
}

impl Ring {
    pub fn new(fraction: Option<f32>) -> Self {
        Self {
            fraction: fraction.map(|f| f.clamp(0.0, 1.0)),
            tone: Tone::Accent,
            size: None,
            label: String::new(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Its diameter (`progress-ring.width` by default).
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }
}

impl Widget for Ring {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let kit = Kit::of(&ctx);
        let ring = &kit.progress_ring;
        let side = self.size.unwrap_or(ring.data.width.max(8.0));
        let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
        if ui.is_rect_visible(rect) {
            let arc = ring.style_toned(self.tone, State::REST).indicator_color;
            paint_ring(ui, rect, self.fraction, arc);
        }
        let value = self.fraction.map(|f| Value {
            value: (f64::from(f) * 100.0).round(),
            min: 0.0,
            max: 100.0,
        });
        let text = self.fraction.map(percent);
        let response = if self.label.is_empty() {
            response
        } else {
            response.on_hover_text(match &text {
                Some(v) => format!("{}: {v}", self.label),
                None => self.label.clone(),
            })
        };
        access(
            ui,
            &response,
            None,
            &self.label,
            value.as_ref(),
            text.as_deref(),
        );
        response
    }
}

/// A value against thresholds (disk space, memory), not progress: a bar
/// whose tone follows the highest threshold reached. AccessKit role
/// `Meter`.
#[derive(Clone, Debug)]
#[must_use = "add it with `ui.add`"]
pub struct Meter {
    value: f64,
    min: f64,
    max: f64,
    thresholds: Vec<(f64, Tone)>,
    label: String,
    text: Option<String>,
    width: Option<f32>,
}

impl Meter {
    /// `value` between `min` and `max`.
    pub fn new(value: f64, min: f64, max: f64) -> Self {
        Self {
            value,
            min,
            max: if max > min { max } else { min + 1.0 },
            thresholds: Vec::new(),
            label: String::new(),
            text: None,
            width: None,
        }
    }

    /// From `at` on, the meter takes `tone`.
    pub fn threshold(mut self, at: f64, tone: Tone) -> Self {
        self.thresholds.push((at, tone));
        self.thresholds.sort_by(|a, b| a.0.total_cmp(&b.0));
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Its value as text ("72 GB of 100 GB").
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The tone at its value.
    pub fn tone(&self) -> Tone {
        self.thresholds
            .iter()
            .rev()
            .find(|(at, _)| self.value >= *at)
            .map_or(Tone::Accent, |(_, tone)| *tone)
    }
}

impl Widget for Meter {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let kit = Kit::of(&ctx);
        let thick = kit.meter.data.height.max(1.0);
        let width = self
            .width
            .unwrap_or_else(|| ui.available_width())
            .max(thick * 4.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, thick), Sense::hover());
        let span = self.max - self.min;
        let at =
            |v: f64| rect.left() + rect.width() * ((v - self.min) / span).clamp(0.0, 1.0) as f32;
        if ui.is_rect_visible(rect) {
            let cx = KitCx {
                painter: ui.painter(),
                ppp: ctx.pixels_per_point(),
            };
            let part = |rect: Rect| Part {
                rect,
                room: rect,
                state: State::REST,
                label: None,
            };
            kit.meter.paint_part(&cx, &part(rect));
            let x = at(self.value);
            if x > rect.left() {
                let fill = Rect::from_min_max(rect.min, pos2(x, rect.bottom()));
                paint_fill(&kit, &cx, fill, rect, self.tone());
            }
            for (v, _) in &self.thresholds {
                let x = at(*v);
                let tick = Rect::from_min_max(pos2(x, rect.top()), pos2(x, rect.bottom()));
                kit.meter_tick.paint_part(&cx, &part(tick));
            }
        }
        let text = self
            .text
            .clone()
            .unwrap_or_else(|| percent(((self.value - self.min) / span) as f32));
        let response = if self.label.is_empty() {
            response
        } else {
            response.on_hover_text(format!("{}: {text}", self.label))
        };
        let value = Value {
            value: self.value,
            min: self.min,
            max: self.max,
        };
        access(
            ui,
            &response,
            Some(Role::Meter),
            &self.label,
            Some(&value),
            Some(&text),
        );
        response
    }
}

/// A spinner, with a label after it; [`Spinner::delayed`] shows it only
/// after `motion.spinner.delay`, and keeps it `motion.spinner.min`.
#[derive(Clone, Debug)]
#[must_use = "add it with `ui.add`"]
pub struct Spinner {
    label: String,
    size: Option<f32>,
    tone: Tone,
    busy: Option<(Busy, bool)>,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            label: String::new(),
            size: None,
            tone: Tone::Accent,
            busy: None,
        }
    }

    /// The text after it ("Loading previews"), also its accessible name.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Shown only for work going on (`busy`) since `since` (motion clock
    /// seconds) for longer than the delay, then for the minimum. The
    /// space stays taken while hidden, so nothing around it moves.
    pub fn delayed(mut self, id: Id, since: f64, busy: bool) -> Self {
        self.busy = Some((Busy::new(id, since), busy));
        self
    }
}

/// Paints a spinner's arc in `rect`, in `color` over `spinner`'s track.
pub fn paint_spinner(ui: &Ui, rect: Rect, color: Color32) {
    paint_spinner_on(ui.painter(), rect, color);
}

/// [`paint_spinner`] with a painter of its own (another layer, a clip).
pub fn paint_spinner_on(painter: &Painter, rect: Rect, color: Color32) {
    let ctx = painter.ctx();
    let t = Tokens::of(ctx);
    let track = Kit::of(ctx).spinner.style(State::REST).fill;
    let side = rect.width().min(rect.height());
    let width = (side / 7.0).max(1.5);
    let radius = side / 2.0 - width / 2.0;
    let phase = motion::phase(ctx, f64::from(t.motion.spinner_turn), 0.125);
    painter.circle_stroke(rect.center(), radius, Stroke::new(width, track));
    painter.line(
        arc(rect.center(), radius, -TAU / 4.0 + phase * TAU, TAU * 0.3),
        Stroke::new(width, color),
    );
    motion::animate(ctx);
}

impl Widget for Spinner {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let kit = Kit::of(&ctx);
        let spinner = &kit.spinner;
        let label_color = spinner.style(State::REST).text;
        let side = self.size.unwrap_or(spinner.data.width.max(8.0));
        let visible = match &self.busy {
            Some((busy, on)) => busy.visible(&ctx, *on),
            None => true,
        };
        let font = FontId::proportional(t.font.body);
        let galley = (!self.label.is_empty()).then(|| {
            ui.painter()
                .layout_no_wrap(self.label.clone(), font, label_color)
        });
        let text_w = galley.as_ref().map_or(0.0, |g| g.size().x + t.space.s);
        let height = galley.as_ref().map_or(side, |g| g.size().y.max(side));
        let (rect, response) = ui.allocate_exact_size(vec2(side + text_w, height), Sense::hover());
        if visible && ui.is_rect_visible(rect) {
            let icon = Rect::from_center_size(
                pos2(rect.left() + side / 2.0, rect.center().y),
                vec2(side, side),
            );
            let arc = spinner.style_toned(self.tone, State::REST).indicator_color;
            paint_spinner(ui, icon, arc);
            if let Some(g) = galley {
                let at = pos2(icon.right() + t.space.s, rect.center().y - g.size().y / 2.0);
                ui.painter().galley(at, g, label_color);
            }
        }
        if visible {
            let name = if self.label.is_empty() {
                crate::strings::tr(&ctx, &crate::strings::BUSY)
            } else {
                self.label.clone()
            };
            access(ui, &response, None, &name, None, None);
        }
        response
    }
}
