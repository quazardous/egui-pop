//! The viewport widget: draws a view of an image and turns gestures into
//! changes of the [`View`], reported as [`Signal`]s.
//!
//! The application owns the view (per window or per document) and the
//! [`ViewportState`] (what the widget keeps between frames, its hooks
//! included). Every change the widget makes goes through the
//! `before_view` hook and is reported as a signal with its phase, so an
//! application that keeps the view in a store records a pan as one
//! gesture, and a cancelled pan as nothing.

use std::collections::VecDeque;

use egui::{
    Color32, Id, Key, Pos2, Response, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};

use crate::debug;
use crate::gestures::{self, DoubleClick, Gestures, WheelAction};
use crate::hooks::{Cause, Hooks, TRACE_KEPT, TraceEntry, Verdict, ViewChange};
use crate::tokens::ViewportTokens;
use crate::view::{self, Mapping, View, Zoom};

/// Where a change stands in its gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// A gesture starts (a drag); the value is the one before it.
    Begin,
    /// A gesture goes on.
    Update,
    /// A gesture ends; the value is the final one.
    End,
    /// A gesture is cancelled; the value is the one before it, restored.
    Cancel,
    /// A change in one step (a wheel notch, a key, a double click, a
    /// command).
    Once,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Begin => "begin",
            Phase::Update => "update",
            Phase::End => "end",
            Phase::Cancel => "cancel",
            Phase::Once => "once",
        }
    }
}

/// What the viewport tells the application.
#[derive(Clone, Debug, PartialEq)]
pub enum Signal {
    /// The view changed.
    View {
        view: View,
        phase: Phase,
        cause: Cause,
    },
}

/// What the widget keeps between frames. One per viewport; the
/// application owns it.
#[derive(Debug, Default)]
pub struct ViewportState {
    pan: Option<Pan>,
    /// The wheel landed on 100 % and is held there until it stops.
    detent: bool,
    last_mapping: Option<Mapping>,
    hooks: Hooks,
    trace: VecDeque<TraceEntry>,
    /// Signals from calls between frames, given with the next frame's.
    pending: Vec<Signal>,
    now: f64,
}

#[derive(Clone, Copy, Debug)]
struct Pan {
    start: View,
}

impl ViewportState {
    /// An empty state.
    pub fn new() -> ViewportState {
        ViewportState::default()
    }

    /// The mapping of the last frame drawn.
    pub fn mapping(&self) -> Option<Mapping> {
        self.last_mapping
    }

    /// Whether a gesture is in progress.
    pub fn in_gesture(&self) -> bool {
        self.pan.is_some()
    }

    /// The hooks, to set or remove.
    pub fn hooks_mut(&mut self) -> &mut Hooks {
        &mut self.hooks
    }

    /// The hooks.
    pub fn hooks(&self) -> &Hooks {
        &self.hooks
    }

    /// What the hooks decided lately, oldest first.
    pub fn trace(&self) -> impl Iterator<Item = &TraceEntry> {
        self.trace.iter()
    }

    pub(crate) fn record(&mut self, hook: &'static str, verdict: &'static str, detail: String) {
        if self.trace.len() == TRACE_KEPT {
            self.trace.pop_front();
        }
        self.trace.push_back(TraceEntry {
            time: self.now,
            hook,
            verdict,
            detail,
        });
    }

    /// Changes the view from outside the widget (a menu's "zoom in"),
    /// through the `before_view` hook like a gesture. Returns whether the
    /// view changed; the signal comes with the next frame's output.
    pub fn propose(&mut self, view: &mut View, to: View) -> bool {
        let Some(m) = self.last_mapping else {
            *view = to;
            return true;
        };
        match self.ask(view, to, Cause::Command, &m) {
            Some(to) if to != *view => {
                *view = to;
                self.pending.push(Signal::View {
                    view: to,
                    phase: Phase::Once,
                    cause: Cause::Command,
                });
                true
            }
            _ => false,
        }
    }

    /// One ladder step up around the centre (a command).
    pub fn zoom_in(&mut self, view: &mut View) -> bool {
        self.zoom_to_with(view, view::zoom_in)
    }

    /// One ladder step down around the centre (a command).
    pub fn zoom_out(&mut self, view: &mut View) -> bool {
        self.zoom_to_with(view, view::zoom_out)
    }

    fn zoom_to_with(&mut self, view: &mut View, f: fn(f64) -> f64) -> bool {
        let Some(m) = self.last_mapping else {
            return false;
        };
        let to = m.zoom_around(m.rect.center(), f(m.zoom));
        self.propose(view, to)
    }

    /// The `before_view` hook's answer for a proposed change, traced.
    fn ask(&mut self, from: &View, to: View, cause: Cause, m: &Mapping) -> Option<View> {
        let Some(hook) = self.hooks.before_view.as_mut() else {
            return Some(to);
        };
        let change = ViewChange {
            from: *from,
            to,
            cause,
        };
        match hook(&change, m) {
            Verdict::Allow => Some(to),
            Verdict::Veto => {
                self.record("before_view", "veto", format!("{} to {to:?}", cause.name()));
                None
            }
            Verdict::Redirect(v) => {
                self.record(
                    "before_view",
                    "redirect",
                    format!("{} to {to:?}, given {v:?}", cause.name()),
                );
                Some(v)
            }
        }
    }
}

/// What a frame of the viewport gives back.
#[derive(Debug)]
pub struct ViewportOutput {
    pub response: Response,
    /// The mapping used this frame (`None` without an image size).
    pub mapping: Option<Mapping>,
    /// The image pixel under the pointer, by the drawing's own rule.
    pub hover_pixel: Option<[i64; 2]>,
    /// What changed this frame, in order.
    pub signals: Vec<Signal>,
}

/// The viewport widget, built per frame.
#[derive(Debug)]
pub struct Viewport {
    id: Id,
    name: String,
    image_size: Option<[u32; 2]>,
    label: String,
    pixel_grid: bool,
    size: Option<Vec2>,
}

/// From this zoom up, the pixel grid shows (when enabled).
pub const GRID_FROM_ZOOM: f64 = 8.0;

/// One frame's working set: the view, its mapping, what changed.
struct Frame<'a> {
    state: &'a mut ViewportState,
    view: &'a mut View,
    mapping: Mapping,
    signals: Vec<Signal>,
}

impl Frame<'_> {
    /// Proposes a change; applies what the hook allows and signals it.
    fn change(&mut self, to: View, cause: Cause, phase: Phase) -> bool {
        let to = if phase == Phase::Cancel {
            Some(to)
        } else {
            self.state.ask(self.view, to, cause, &self.mapping)
        };
        let Some(to) = to else { return false };
        if to == *self.view && phase != Phase::Cancel {
            return false;
        }
        *self.view = to;
        self.mapping = Mapping::new(
            self.view,
            self.mapping.image,
            self.mapping.rect,
            self.mapping.ppp,
        );
        self.signals.push(Signal::View {
            view: to,
            phase,
            cause,
        });
        true
    }

    fn signal(&mut self, phase: Phase, cause: Cause) {
        self.signals.push(Signal::View {
            view: *self.view,
            phase,
            cause,
        });
    }
}

impl Viewport {
    /// A viewport; `id_salt` tells it apart from others in the same `Ui`
    /// and names it in the debug bridge.
    pub fn new(id_salt: impl egui::AsId) -> Viewport {
        Viewport {
            name: format!("{id_salt:?}").trim_matches('"').to_string(),
            id: Id::new(id_salt),
            image_size: None,
            label: "Image viewport".into(),
            pixel_grid: true,
            size: None,
        }
    }

    /// The image's size in pixels.
    pub fn image_size(mut self, size: [u32; 2]) -> Viewport {
        self.image_size = Some(size);
        self
    }

    /// The accessible name (default "Image viewport").
    pub fn label(mut self, label: impl Into<String>) -> Viewport {
        self.label = label.into();
        self
    }

    /// Draws the pixel grid from [`GRID_FROM_ZOOM`] up (default on).
    pub fn pixel_grid(mut self, on: bool) -> Viewport {
        self.pixel_grid = on;
        self
    }

    /// The widget's size in points (default: all the space available).
    pub fn size(mut self, size: Vec2) -> Viewport {
        self.size = Some(size);
        self
    }

    /// Draws the viewport and handles its gestures.
    pub fn show(self, ui: &mut Ui, state: &mut ViewportState, view: &mut View) -> ViewportOutput {
        let size = self.size.unwrap_or_else(|| ui.available_size());
        let (_, rect) = ui.allocate_space(size);
        let response = ui.interact(
            rect,
            ui.make_persistent_id(self.id),
            Sense::click_and_drag(),
        );
        let tokens = ViewportTokens::of(ui.ctx());
        let g = gestures::gestures(ui.ctx());
        let ppp = ui.ctx().pixels_per_point();
        state.now = ui.input(|i| i.time);
        let mut signals = std::mem::take(&mut state.pending);
        ui.painter().rect_filled(rect, 0.0, tokens.background);

        let Some(image) = self.image_size.map(|s| [f64::from(s[0]), f64::from(s[1])]) else {
            name(&response, &self.label, None);
            state.last_mapping = None;
            return ViewportOutput {
                response,
                mapping: None,
                hover_pixel: None,
                signals,
            };
        };

        let mapping = Mapping::new(view, image, rect, ppp);
        let mut f = Frame {
            state,
            view,
            mapping,
            signals: Vec::new(),
        };
        for to in debug::take_commands(ui.ctx(), &self.name) {
            f.change(to, Cause::Command, Phase::Once);
        }
        let pan_key = response.hovered() && ui.input(|i| i.key_down(g.pan_key));
        handle_pan(ui, &response, &g, pan_key, &mut f);
        handle_double_click(&response, &g, &mut f);
        handle_wheel(ui, &response, &g, &mut f);
        handle_keys(ui, &response, &g, &mut f);
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        let Frame {
            state,
            view,
            mapping,
            signals: frame_signals,
        } = f;
        signals.extend(frame_signals);

        // The image.
        let image_rect = mapping.image_rect_to_screen([0.0, 0.0, image[0], image[1]]);
        let painter = ui.painter_at(rect);
        painter.rect_filled(image_rect, 0.0, tokens.backdrop);
        if self.pixel_grid && mapping.zoom >= GRID_FROM_ZOOM {
            draw_grid(&painter, &mapping, tokens.grid);
        }
        painter.rect_stroke(
            image_rect,
            0.0,
            Stroke::new(1.0 / ppp, tokens.frame),
            StrokeKind::Outside,
        );
        if let Some(after) = state.hooks.after_paint.as_mut() {
            after(&painter, &mapping);
        }
        if response.has_focus() {
            let t = egui_pop_core::theme::Tokens::of(ui.ctx());
            painter.rect_stroke(
                rect.shrink(1.0),
                0.0,
                Stroke::new(2.0, t.focus_ring),
                StrokeKind::Inside,
            );
        }

        let hover_pixel = response.hover_pos().and_then(|p| mapping.pixel_at(p));
        name(&response, &self.label, Some(mapping.zoom));
        state.last_mapping = Some(mapping);
        debug::publish(ui.ctx(), &self.name, state, view, &mapping, &signals);
        ViewportOutput {
            response,
            mapping: Some(mapping),
            hover_pixel,
            signals,
        }
    }
}

fn handle_pan(ui: &Ui, response: &Response, g: &Gestures, pan_key: bool, f: &mut Frame<'_>) {
    let starts = response.drag_started_by(g.pan_button)
        || response.drag_started_by(g.pan_button_anywhere)
        || (pan_key && response.drag_started());
    if starts && f.state.pan.is_none() {
        f.state.pan = Some(Pan { start: *f.view });
        f.signal(Phase::Begin, Cause::Pan);
    }
    let Some(pan) = f.state.pan else { return };
    if ui.input(|i| i.key_pressed(g.cancel)) {
        f.state.pan = None;
        f.change(pan.start, Cause::Pan, Phase::Cancel);
        return;
    }
    let delta = response.drag_delta();
    if delta != Vec2::ZERO {
        let to = f.mapping.panned(delta);
        f.change(to, Cause::Pan, Phase::Update);
    }
    if response.drag_stopped() || !response.dragged() && !response.drag_started() {
        f.state.pan = None;
        f.signal(Phase::End, Cause::Pan);
    }
}

fn handle_double_click(response: &Response, g: &Gestures, f: &mut Frame<'_>) {
    if !response.double_clicked() {
        return;
    }
    let at = response.hover_pos().unwrap_or(f.mapping.rect.center());
    let fit = f.view.is_fit();
    let m = f.mapping;
    let action = g.double_click;
    let mut default = || match action {
        DoubleClick::Nothing => None,
        DoubleClick::Fit => Some(View::fit()),
        DoubleClick::FitOrActual => Some(if fit {
            m.zoom_around(at, 1.0)
        } else {
            View::fit()
        }),
    };
    let to = if let Some(run) = f.state.hooks.on_double_click.as_mut() {
        let to = run(at, &m, &mut default);
        f.state
            .record("on_double_click", "run", format!("gave {to:?}"));
        to
    } else {
        default()
    };
    if let Some(to) = to {
        f.change(to, Cause::DoubleClick, Phase::Once);
    }
}

fn handle_wheel(ui: &Ui, response: &Response, g: &Gestures, f: &mut Frame<'_>) {
    if !response.hovered() {
        f.state.detent = false;
        return;
    }
    let (scroll, pinch, modifiers) =
        ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers));
    // The canvas takes the wheel: the panel around it does not scroll.
    ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    let pointer = response.hover_pos().unwrap_or(f.mapping.rect.center());
    let other = !g.wheel_other.is_none() && modifiers.matches_logically(g.wheel_other);
    let zooms = (g.wheel == WheelAction::Zoom) != other;
    if pinch != 1.0 {
        let z = view::wheel_zoom(
            f.mapping.zoom,
            (f64::from(pinch).ln() / view::WHEEL_RATE) as f32,
        );
        let to = f.mapping.zoom_around(pointer, z);
        f.change(to, Cause::Pinch, Phase::Once);
    } else if scroll == Vec2::ZERO {
        f.state.detent = false;
    } else if zooms {
        if f.state.detent {
            // Held on 100 % until the wheel stops.
            return;
        }
        let dy = if scroll.y != 0.0 { scroll.y } else { scroll.x };
        let z = view::wheel_zoom(f.mapping.zoom, dy);
        f.state.detent = z == 1.0 && f.mapping.zoom != 1.0;
        let to = f.mapping.zoom_around(pointer, z);
        f.change(to, Cause::Wheel, Phase::Once);
    } else {
        let to = f.mapping.panned(scroll);
        f.change(to, Cause::Wheel, Phase::Once);
    }
}

fn handle_keys(ui: &Ui, response: &Response, g: &Gestures, f: &mut Frame<'_>) {
    if !response.has_focus() || f.state.pan.is_some() {
        return;
    }
    let m = f.mapping;
    let center = m.rect.center();
    let pressed = |k: Key| ui.input(|i| i.key_pressed(k));
    let to = if g.zoom_in.iter().any(|k| pressed(*k)) {
        Some(m.zoom_around(center, view::zoom_in(m.zoom)))
    } else if g.zoom_out.iter().any(|k| pressed(*k)) {
        Some(m.zoom_around(center, view::zoom_out(m.zoom)))
    } else if pressed(g.fit) {
        Some(View::fit())
    } else if pressed(g.actual) {
        Some(m.zoom_around(center, 1.0))
    } else if pressed(g.double) {
        Some(m.zoom_around(center, 2.0))
    } else {
        let coarse = !g.coarse.is_none() && ui.input(|i| i.modifiers.matches_logically(g.coarse));
        let step = m.rect.size() * if coarse { 0.5 } else { 0.1 };
        let mut d = Vec2::ZERO;
        if pressed(Key::ArrowLeft) {
            d.x += step.x;
        }
        if pressed(Key::ArrowRight) {
            d.x -= step.x;
        }
        if pressed(Key::ArrowUp) {
            d.y += step.y;
        }
        if pressed(Key::ArrowDown) {
            d.y -= step.y;
        }
        (d != Vec2::ZERO).then(|| m.panned(d))
    };
    if let Some(to) = to {
        f.change(to, Cause::Key, Phase::Once);
    }
}

/// The accessible name, with the zoom as the value.
fn name(response: &Response, label: &str, zoom: Option<f64>) {
    let label = label.to_string();
    response.widget_info(move || {
        let mut info = WidgetInfo::labeled(WidgetType::Other, true, &label);
        if let Some(z) = zoom {
            info.value = Some(z * 100.0);
            info.current_text_value = Some(format!("{:.0} %", z * 100.0));
        }
        info
    });
}

/// Lines between image pixels, over the visible part.
fn draw_grid(painter: &egui::Painter, m: &Mapping, color: Color32) {
    let Some(v) = m.visible() else { return };
    let stroke = Stroke::new(1.0 / m.ppp, color);
    let (x0, x1) = (v[0].floor() as i64, v[2].ceil() as i64);
    let (y0, y1) = (v[1].floor() as i64, v[3].ceil() as i64);
    if (x1 - x0) > 1000 || (y1 - y0) > 1000 {
        return;
    }
    let top = m.image_to_screen([0.0, v[1]]).y;
    let bottom = m.image_to_screen([0.0, v[3]]).y;
    let left = m.image_to_screen([v[0], 0.0]).x;
    let right = m.image_to_screen([v[2], 0.0]).x;
    for x in x0..=x1 {
        let sx = m.image_to_screen([x as f64, 0.0]).x;
        painter.line_segment([Pos2::new(sx, top), Pos2::new(sx, bottom)], stroke);
    }
    for y in y0..=y1 {
        let sy = m.image_to_screen([0.0, y as f64]).y;
        painter.line_segment([Pos2::new(left, sy), Pos2::new(right, sy)], stroke);
    }
}

/// The zoom of a view as shown, in percent (fit resolved by `mapping`).
pub fn zoom_percent(view: &View, mapping: Option<&Mapping>) -> f64 {
    match (view.zoom, mapping) {
        (Zoom::Px(z), _) => z * 100.0,
        (Zoom::Fit, Some(m)) => m.zoom * 100.0,
        (Zoom::Fit, None) => 100.0,
    }
}
