//! [`Transport`]: a row of transport buttons over a [`TimelineState`]
//! (go to start, step back, play backward, stop, play or pause, step
//! forward, go to end, loop) and the playhead's text beside them. Each
//! button does what its key does on the timeline: with an application and
//! the timeline's id it runs the same `timeline.*` command (so the palette,
//! the history and agents see one thing), else it changes the state
//! directly.

use egui::accesskit::Role;
use egui::{
    Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, WidgetInfo, pos2, vec2,
};
use egui_pop_api::AppHandle;
use egui_pop_core::strings::{StrKey, tr, tr_args};
use egui_pop_model::ids::CommandId;
use serde_json::json;

use super::plugin::{
    CMD_GO_TO_END, CMD_GO_TO_START, CMD_PAUSE, CMD_SHUTTLE_BACKWARD, CMD_STEP_BACKWARD,
    CMD_STEP_FORWARD, CMD_TOGGLE_LOOP, CMD_TOGGLE_PLAY,
};
use super::state::TimelineState;
use super::{Op, apply};
use crate::strings as txt;

/// A transport button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TransportButton {
    /// To the bounds' start (Home).
    Start,
    /// One unit back, pausing (Left).
    StepBack,
    /// Plays backward, twice as fast again (J).
    PlayBackward,
    /// Pauses, the speed back to 1 (K).
    Stop,
    /// Plays forward at 1, or pauses (Space).
    PlayPause,
    /// One unit on, pausing (Right).
    StepForward,
    /// To the bounds' end (End).
    End,
    /// Loops the range, or no longer (Ctrl+L).
    Loop,
}

impl TransportButton {
    /// A video editor's row, in this order.
    pub const ALL: [TransportButton; 8] = [
        TransportButton::Start,
        TransportButton::StepBack,
        TransportButton::PlayBackward,
        TransportButton::Stop,
        TransportButton::PlayPause,
        TransportButton::StepForward,
        TransportButton::End,
        TransportButton::Loop,
    ];

    /// The `timeline.*` command it runs, its key's.
    pub fn command(self) -> CommandId {
        match self {
            TransportButton::Start => CMD_GO_TO_START,
            TransportButton::StepBack => CMD_STEP_BACKWARD,
            TransportButton::PlayBackward => CMD_SHUTTLE_BACKWARD,
            TransportButton::Stop => CMD_PAUSE,
            TransportButton::PlayPause => CMD_TOGGLE_PLAY,
            TransportButton::StepForward => CMD_STEP_FORWARD,
            TransportButton::End => CMD_GO_TO_END,
            TransportButton::Loop => CMD_TOGGLE_LOOP,
        }
    }

    /// What it does to the state.
    fn op(self) -> Op {
        match self {
            TransportButton::Start => Op::Home,
            TransportButton::StepBack => Op::Step {
                forward: false,
                big: false,
            },
            TransportButton::PlayBackward => Op::Shuttle(false),
            TransportButton::Stop => Op::Pause,
            TransportButton::PlayPause => Op::Toggle,
            TransportButton::StepForward => Op::Step {
                forward: true,
                big: false,
            },
            TransportButton::End => Op::End,
            TransportButton::Loop => Op::ToggleLoop,
        }
    }

    /// Its name (playing or not) and its keys, for the tooltip.
    fn name_and_keys(self, playing: bool) -> (&'static StrKey, &'static str) {
        match self {
            TransportButton::Start => (&txt::TRANSPORT_START, "Home"),
            TransportButton::StepBack => (&txt::TRANSPORT_STEP_BACK, "←"),
            TransportButton::PlayBackward => (&txt::TRANSPORT_PLAY_BACKWARD, "J"),
            TransportButton::Stop => (&txt::TRANSPORT_STOP, "K"),
            TransportButton::PlayPause if playing => (&txt::TRANSPORT_PAUSE, "Space, K"),
            TransportButton::PlayPause => (&txt::TRANSPORT_PLAY, "Space, L"),
            TransportButton::StepForward => (&txt::TRANSPORT_STEP_FORWARD, "→"),
            TransportButton::End => (&txt::TRANSPORT_END, "End"),
            TransportButton::Loop => (&txt::TRANSPORT_LOOP, "Ctrl+L"),
        }
    }
}

/// What a transport did this pass.
#[derive(Debug)]
#[non_exhaustive]
pub struct TransportOutput {
    /// The whole row.
    pub response: Response,
    /// The button clicked, if one was.
    pub clicked: Option<TransportButton>,
}

/// A row of transport buttons over a timeline's state, the playhead's text
/// beside them.
///
/// ```no_run
/// use egui_pop_views::timeline::{Timeline, TimelineState, Transport};
///
/// fn ui(ui: &mut egui::Ui, app: &egui_pop_api::AppHandle, seq: &mut TimelineState) {
///     // The buttons run the `timeline.*` commands on the timeline `sequence`.
///     Transport::new(seq).app(app).timeline("sequence").show(ui);
///     Timeline::new(seq).id("sequence").app(app).show(ui);
/// }
/// ```
#[must_use = "call `show` to draw it"]
pub struct Transport<'a> {
    state: &'a mut TimelineState,
    app: Option<&'a AppHandle>,
    timeline: Option<&'a str>,
    buttons: &'a [TransportButton],
    timecode: bool,
}

impl std::fmt::Debug for Transport<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("timeline", &self.timeline)
            .field("buttons", &self.buttons)
            .finish_non_exhaustive()
    }
}

impl<'a> Transport<'a> {
    pub fn new(state: &'a mut TimelineState) -> Self {
        Self {
            state,
            app: None,
            timeline: None,
            buttons: &TransportButton::ALL,
            timecode: true,
        }
    }

    /// The application whose `timeline.*` commands the buttons run (with
    /// [`timeline`](Self::timeline)): each click is then the command, a
    /// step of the history where the key's is.
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    /// The timeline's id ([`Timeline::id`](super::Timeline::id)), the
    /// commands' `timeline`.
    pub fn timeline(mut self, id: &'a str) -> Self {
        self.timeline = Some(id);
        self
    }

    /// The buttons, in order ([`TransportButton::ALL`] by default).
    pub fn buttons(mut self, buttons: &'a [TransportButton]) -> Self {
        self.buttons = buttons;
        self
    }

    /// The playhead's text after the buttons (on by default).
    pub fn timecode(mut self, on: bool) -> Self {
        self.timecode = on;
        self
    }

    pub fn show(self, ui: &mut Ui) -> TransportOutput {
        let Transport {
            state,
            app,
            timeline,
            buttons,
            timecode,
        } = self;
        let ctx = ui.ctx().clone();
        let mut clicked = None;
        let row = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let playing = state.is_playing();
            for &b in buttons {
                let (name, keys) = b.name_and_keys(playing);
                let name = tr(&ctx, name);
                let selected = match b {
                    TransportButton::PlayBackward => playing && state.speed < 0.0,
                    TransportButton::Loop => state.looping,
                    _ => false,
                };
                let r = button(ui, b, &name, selected, playing).on_hover_text(tr_args(
                    &ctx,
                    &txt::TRANSPORT_KEYS,
                    &[("action", &name), ("keys", keys)],
                ));
                if r.clicked() {
                    clicked = Some(b);
                }
            }
            if timecode {
                ui.add_space(6.0);
                ui.monospace(state.scale.format(state.playhead));
            }
        });
        if let Some(b) = clicked {
            match (app, timeline) {
                (Some(app), Some(id)) => app.run(b.command(), json!({ "timeline": id })),
                _ => {
                    apply(state, b.op());
                }
            }
        }
        TransportOutput {
            response: row.response,
            clicked,
        }
    }
}

/// A transport button: the theme's button look, its glyph painted.
fn button(ui: &mut Ui, b: TransportButton, name: &str, selected: bool, playing: bool) -> Response {
    let h = ui.spacing().interact_size.y;
    let (rect, r) = ui.allocate_exact_size(vec2(h * 1.3, h), Sense::click());
    let enabled = ui.is_enabled();
    r.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, name));
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact_selectable(&r, selected);
        let p = ui.painter();
        p.rect(
            rect.expand(v.expansion),
            v.corner_radius,
            v.weak_bg_fill,
            v.bg_stroke,
            egui::StrokeKind::Inside,
        );
        glyph(p, rect, b, playing, v.fg_stroke.color);
    }
    r
}

/// A triangle pointing right (or left) in `r`.
fn triangle(p: &Painter, r: Rect, right: bool, color: Color32) {
    let pts = if right {
        vec![r.left_top(), r.right_center(), r.left_bottom()]
    } else {
        vec![r.right_top(), r.left_center(), r.right_bottom()]
    };
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A bar from `top` down `h`, at `x`.
fn bar(p: &Painter, x: f32, top: f32, h: f32, w: f32, color: Color32) {
    p.rect_filled(Rect::from_min_size(pos2(x, top), vec2(w, h)), 0.0, color);
}

fn glyph(p: &Painter, rect: Rect, b: TransportButton, playing: bool, color: Color32) {
    let s = (rect.height() * 0.45).round();
    let c = rect.center();
    let g = Rect::from_center_size(c, vec2(s, s));
    let w = (s / 5.0).max(1.5);
    let half = |left: bool| {
        let x = if left { g.left() } else { c.x };
        Rect::from_min_size(pos2(x, g.top()), vec2(s / 2.0, s))
    };
    match b {
        TransportButton::Start => {
            bar(p, g.left() - w, g.top(), s, w, color);
            triangle(p, half(true), false, color);
            triangle(p, half(false), false, color);
        }
        TransportButton::End => {
            triangle(p, half(true), true, color);
            triangle(p, half(false), true, color);
            bar(p, g.right(), g.top(), s, w, color);
        }
        TransportButton::StepBack => {
            let t = Rect::from_min_size(g.left_top(), vec2(s * 0.7, s));
            triangle(p, t, false, color);
            bar(p, g.right() - w, g.top(), s, w, color);
        }
        TransportButton::StepForward => {
            bar(p, g.left(), g.top(), s, w, color);
            let t = Rect::from_min_size(pos2(g.right() - s * 0.7, g.top()), vec2(s * 0.7, s));
            triangle(p, t, true, color);
        }
        TransportButton::PlayBackward => {
            triangle(p, g.shrink2(vec2(s * 0.1, 0.0)), false, color);
        }
        TransportButton::PlayPause if playing => {
            bar(p, g.left() + s * 0.1, g.top(), s, s * 0.3, color);
            bar(p, g.right() - s * 0.4, g.top(), s, s * 0.3, color);
        }
        TransportButton::PlayPause => {
            triangle(p, g.shrink2(vec2(s * 0.1, 0.0)), true, color);
        }
        TransportButton::Stop => {
            p.rect_filled(g.shrink(s * 0.08), 1.0, color);
        }
        TransportButton::Loop => {
            // A ring, its arrowhead on top going right.
            let radius = s * 0.45;
            p.circle_stroke(c, radius, Stroke::new(w * 0.8, color));
            let tip: Pos2 = c + vec2(s * 0.12, -radius);
            let head = Rect::from_center_size(tip, vec2(s * 0.35, s * 0.4));
            triangle(p, head, true, color);
        }
    }
}
