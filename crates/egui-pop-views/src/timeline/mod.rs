//! Timeline: the [`Timeline`] widget, a ruler of time with a playhead, a
//! range, markers and the transport keys, over a [`TimeScale`] (seconds,
//! frames and timecode, bars and beats).
//!
//! ```no_run
//! use egui_pop_views::timeline::{Marker, TimeScale, Timeline, TimelineState};
//!
//! fn ui(ui: &mut egui::Ui, song: &mut TimelineState, dt: f64) {
//!     // The application moves the time; the widget asks for it.
//!     song.advance(dt);
//!     let out = Timeline::new(song).id("song").show(ui);
//!     if out.response.has_focus() && song.is_playing() {
//!         ui.ctx().request_repaint();
//!     }
//! }
//!
//! let mut song = TimelineState::new(TimeScale::beats(128.0), 240.0);
//! song.add_marker(Marker::new(0, 30.0, "Drop"));
//! ```
//!
//! - **Time** is an `f64` in seconds everywhere; a [`TimeScale`] only reads
//!   and writes it in its units, spaces the ruler and gives the grid.
//! - **State**: the application keeps a [`TimelineState`] (serializable):
//!   the view, the playhead, the range and its loop, the markers, and the
//!   play state and speed the user asked for. Playing is the application's
//!   job: it moves the playhead ([`TimelineState::advance`] does it the
//!   usual way) and repaints while it plays.
//! - **Pointer**: a press on the ruler or the lanes moves the playhead and a
//!   drag scrubs; Shift+drag selects a range; Ctrl+drag or the middle
//!   button pans; the wheel pans, Ctrl+wheel and a pinch zoom around the
//!   pointer. A marker is clicked to select it, dragged to move it; a double
//!   click on the markers' lane adds one. Snapping goes to the grid (the
//!   ruler's small ticks), the markers, the playhead and the range's ends;
//!   Alt turns it off while held ([`Snap`] chooses the targets).
//! - **Keys**, while it has the focus (a Tab stop): Space plays or pauses;
//!   J, K, L shuttle (J or L again doubles the speed, K pauses, K held with
//!   J or L steps one unit); Left and Right step one unit, Shift for a
//!   bigger one ([`TimeScale::step`]); Home and End; I and O set the range's
//!   in and out points, Shift+I and Shift+O go to them, Alt+X clears the
//!   range, Ctrl+L loops it; M adds a marker at the playhead; Up and Down
//!   select the previous and next marker; Ctrl+Left and Ctrl+Right move the
//!   selected one; Delete removes it, Escape lets it go; `+` and `-` zoom.
//!   A key an application binds on the timeline's surface ([`SURFACE`]) runs
//!   its command instead.
//! - **History**: with [`Timeline::history`], each change of the document
//!   (adding, moving, removing a marker, setting or clearing the range,
//!   looping) is one step of egui-pop's history, undone with Ctrl+Z; the
//!   playhead, the view and playing are not. Without it, the output lists
//!   the changes ([`TimelineOutput::edits`]) for the application's own.
//! - **Accessibility**: the timeline is a slider whose value is the
//!   playhead's text; Up and Down reach the markers, each named.
//! - **Agents**: a timeline given an [`id`](Timeline::id) is listed by the
//!   `timeline` collector and driven by the `timeline.*` commands
//!   ([`TimelinePlugin`]).
//! - **Look**: the theme's tokens only, `timeline.*` with fallbacks
//!   ([`TimelineTokens`]).

mod plugin;
pub mod ruler;
mod scale;
mod state;
mod tokens;

use egui::accesskit::{Action, ActionData, Role};
use egui::{
    Color32, Event, EventFilter, FontId, Id, Key, Modifiers, PointerButton, Pos2, Rect, Response,
    Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use egui_pop_api::AppHandle;
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::lock;
use egui_pop_core::strings::{StrKey, tr, tr_args};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::OwnerId;
use egui_pop_model::values::{Change, Descriptor, Set, Value};

pub use plugin::{
    CMD_ADD_MARKER, CMD_PAUSE, CMD_PLAY, CMD_REMOVE_MARKER, CMD_SET_RANGE, CMD_SET_TIME, PLUGIN,
    TimelinePlugin, Timelines, describe,
};
pub use ruler::{Snapped, Ticks};
pub use scale::{FrameRate, Tempo, TimeScale};
pub use state::{
    Edit, MAX_SPEED, Marker, MarkerKind, PlayState, TimeRange, TimeView, TimelineDoc, TimelineState,
};
pub use tokens::TimelineTokens;

use crate::strings as txt;

/// The surface a timeline opens, for key bindings (`timelineFocus`).
pub const SURFACE: &str = "timeline";
/// How near, in points, a time is pulled to a marker, the playhead or a
/// range's end.
pub const SNAP_REACH: f32 = 8.0;

/// What a timeline snaps to; Alt held snaps to nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snap {
    /// The grid, the ruler's small ticks.
    pub grid: bool,
    /// The markers and the range's ends.
    pub markers: bool,
    pub playhead: bool,
}

impl Default for Snap {
    fn default() -> Self {
        Self {
            grid: true,
            markers: true,
            playhead: true,
        }
    }
}

impl Snap {
    /// Nothing: times as the pointer gives them (whole frames on frames).
    pub const OFF: Snap = Snap {
        grid: false,
        markers: false,
        playhead: false,
    };
}

/// What a user, a key or an agent asks of a timeline.
#[derive(Clone, Debug)]
enum Op {
    SetTime(f64),
    /// From the keys: a step, which pauses.
    StepBy(f64),
    Toggle,
    Play(f64),
    Pause,
    Shuttle(bool),
    SetRange(Option<TimeRange>),
    SetLoop(bool),
    AddMarker(Marker),
    /// A marker moved already (a drag), from `from`: its step.
    CommitMove {
        id: u64,
        from: f64,
    },
    MoveMarker(u64, f64),
    RemoveMarker(u64),
}

/// What applying an op did.
#[derive(Debug, Default)]
struct Applied {
    edit: Option<Edit>,
    transport: bool,
    moved: bool,
}

/// Applies `op` to `state`.
fn apply(state: &mut TimelineState, op: Op) -> Applied {
    let mut done = Applied::default();
    let before = (state.play, state.speed, state.playhead);
    match op {
        Op::SetTime(t) => {
            state.set_playhead(t);
        }
        Op::StepBy(by) => {
            state.step_by(by);
        }
        Op::Toggle => state.toggle(),
        Op::Play(speed) => state.play_at(speed),
        Op::Pause => state.pause(),
        Op::Shuttle(forward) => state.shuttle(forward),
        Op::SetRange(range) => {
            let to = range.map(|r| TimeRange::new(state.clamp(r.start), state.clamp(r.end)));
            if to != state.range {
                done.edit = Some(Edit::RangeSet {
                    from: state.range,
                    to,
                });
                state.range = to;
            }
        }
        Op::SetLoop(on) => {
            if on != state.looping {
                state.looping = on;
                done.edit = Some(Edit::LoopSet(on));
            }
        }
        Op::AddMarker(m) => {
            let id = state.add_marker(m);
            state.selected = Some(id);
            done.edit = state.marker(id).cloned().map(Edit::MarkerAdded);
        }
        Op::CommitMove { id, from } => {
            if let Some(m) = state.marker(id)
                && m.time != from
            {
                done.edit = Some(Edit::MarkerMoved {
                    id,
                    from,
                    to: m.time,
                });
            }
        }
        Op::MoveMarker(id, to) => {
            if let Some(from) = state.move_marker(id, to)
                && let Some(m) = state.marker(id)
                && m.time != from
            {
                done.edit = Some(Edit::MarkerMoved {
                    id,
                    from,
                    to: m.time,
                });
            }
        }
        Op::RemoveMarker(id) => {
            done.edit = state.remove_marker(id).map(Edit::MarkerRemoved);
        }
    }
    done.transport = (state.play, state.speed) != (before.0, before.1);
    done.moved = state.playhead != before.2;
    done
}

/// The history step's name for an edit.
fn edit_label(edit: &Edit) -> Option<&'static StrKey> {
    Some(match edit {
        Edit::MarkerAdded(_) => &txt::TIMELINE_ADD_MARKER,
        Edit::MarkerMoved { .. } => &txt::TIMELINE_MOVE_MARKER,
        Edit::MarkerRemoved(_) => &txt::TIMELINE_REMOVE_MARKER,
        Edit::RangeSet { to: Some(_), .. } => &txt::TIMELINE_SET_RANGE,
        Edit::RangeSet { to: None, .. } => &txt::TIMELINE_CLEAR_RANGE,
        Edit::LoopSet(true) => &txt::TIMELINE_LOOP_ON,
        Edit::LoopSet(false) => &txt::TIMELINE_LOOP_OFF,
        Edit::Restored => return None,
    })
}

/// The lanes under the ruler, for what an application draws there (rows of
/// data, clips): where a time is across them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lanes {
    pub rect: Rect,
    pub view: TimeView,
    pub ticks: Ticks,
}

impl Lanes {
    /// The x of `time`.
    pub fn x_of(&self, time: f64) -> f32 {
        self.rect.left() + self.view.x_of(time, self.rect.width())
    }

    /// The time at `x`.
    pub fn time_at(&self, x: f32) -> f64 {
        self.view.time_at(x - self.rect.left(), self.rect.width())
    }
}

/// What a timeline did this pass.
#[derive(Debug)]
pub struct TimelineOutput {
    /// The whole timeline (its keyboard focus, its accessible node).
    pub response: Response,
    /// The playhead moved (the user, a key or an agent; not playing).
    pub playhead_moved: bool,
    /// The playhead is being dragged.
    pub scrubbing: bool,
    /// Play, pause or the speed was asked this pass.
    pub transport_changed: bool,
    pub play: PlayState,
    /// Asked speed, negative backwards.
    pub speed: f64,
    /// Zoomed or panned.
    pub view_changed: bool,
    /// The document's changes this pass (with [`Timeline::history`], each
    /// is a step of the history already).
    pub edits: Vec<Edit>,
}

/// A drag going on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Drag {
    #[default]
    None,
    Scrub,
    Range {
        anchor: f64,
    },
    Pan {
        start: f64,
        x: f32,
    },
    Marker {
        id: u64,
        from: f64,
    },
}

/// What a timeline keeps from one pass to the next.
#[derive(Clone, Debug, Default)]
struct Memory {
    drag: Drag,
    /// The range a Shift+drag is drawing.
    preview: Option<TimeRange>,
    /// The document's text as last written to or read from the store.
    text: Option<String>,
}

/// A timeline drawn over a [`TimelineState`] the application keeps.
#[must_use = "call `show` to draw it"]
pub struct Timeline<'a> {
    state: &'a mut TimelineState,
    id: Option<String>,
    label: Option<String>,
    lanes: Option<f32>,
    width: Option<f32>,
    snap: Snap,
    app: Option<&'a AppHandle>,
    history: Option<String>,
    surface: &'a str,
    take_focus: bool,
    follow: bool,
}

impl std::fmt::Debug for Timeline<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Timeline")
            .field("id", &self.id)
            .field("history", &self.history)
            .finish_non_exhaustive()
    }
}

/// The context of one pass: the output being filled, who acts.
struct Pass<'h> {
    out_edits: Vec<Edit>,
    transport: bool,
    moved: bool,
    history: Option<(&'h AppHandle, &'h str)>,
}

impl Pass<'_> {
    /// Runs `op`, recording its edit (a step of the history when bound).
    fn run(
        &mut self,
        ctx: &egui::Context,
        state: &mut TimelineState,
        mem: &mut Memory,
        op: Op,
        origin: &Origin,
    ) {
        let done = apply(state, op);
        self.transport |= done.transport;
        self.moved |= done.moved;
        if let Some(edit) = done.edit {
            if let (Some((app, path)), Some(label)) = (self.history, edit_label(&edit)) {
                commit(app, path, state, &tr(ctx, label), origin, mem);
            }
            self.out_edits.push(edit);
        }
    }
}

impl<'a> Timeline<'a> {
    pub fn new(state: &'a mut TimelineState) -> Self {
        Self {
            state,
            id: None,
            label: None,
            lanes: None,
            width: None,
            snap: Snap::default(),
            app: None,
            history: None,
            surface: SURFACE,
            take_focus: false,
            follow: true,
        }
    }

    /// Its name for agents (the `timeline` collector, the `timeline.*`
    /// commands' `timeline`) and its key in the context. Without one, it is
    /// keyed by its place in the `Ui`, and agents do not see it.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Its accessible name, instead of "Timeline".
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The height of the lanes under the ruler and the markers (by default,
    /// a control's height).
    pub fn lanes(mut self, height: f32) -> Self {
        self.lanes = Some(height.max(0.0));
        self
    }

    /// Its width (by default, the width available).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn snap(mut self, snap: Snap) -> Self {
        self.snap = snap;
        self
    }

    /// The application, for [`history`](Self::history).
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    /// Keeps the document's part (markers, range, loop) as text at `path` of
    /// the application's value store, declared there if it is not: each
    /// change is one step of its history, and an undo puts the document
    /// back. Needs [`app`](Self::app).
    pub fn history(mut self, path: impl Into<String>) -> Self {
        self.history = Some(path.into());
        self
    }

    /// The focus surface around it, for key bindings ([`SURFACE`] by
    /// default).
    pub fn surface(mut self, surface: &'a str) -> Self {
        self.surface = surface;
        self
    }

    /// It takes the keyboard focus this pass.
    pub fn take_focus(mut self, on: bool) -> Self {
        self.take_focus = on;
        self
    }

    /// While it plays, the view follows the playhead (on by default).
    pub fn follow(mut self, on: bool) -> Self {
        self.follow = on;
        self
    }

    /// Draws it, its lanes empty.
    pub fn show(self, ui: &mut Ui) -> TimelineOutput {
        self.show_with(ui, |_, _| {})
    }

    /// Draws it, `lanes` drawing what goes under the ruler (rows, clips)
    /// before the markers and the playhead are drawn over it.
    pub fn show_with(self, ui: &mut Ui, lanes: impl FnOnce(&mut Ui, &Lanes)) -> TimelineOutput {
        let surface = self.surface;
        egui_pop_core::focus::scope(ui, surface, &[], |ui| self.draw(ui, lanes))
    }

    fn draw(self, ui: &mut Ui, body: impl FnOnce(&mut Ui, &Lanes)) -> TimelineOutput {
        let Timeline {
            state,
            id,
            label,
            lanes,
            width,
            snap,
            app,
            history,
            surface: _,
            take_focus,
            follow,
        } = self;
        let ctx = ui.ctx().clone();
        let tk = TimelineTokens::of(&ctx);
        let key = match &id {
            Some(id) => Id::unique(("egui_pop.timeline", id)),
            None => ui.auto_id_with("egui_pop.timeline"),
        };
        let mut mem: Memory = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
        let history = match (app, history.as_deref()) {
            (Some(app), Some(path)) => Some((app, path)),
            _ => None,
        };
        let mut pass = Pass {
            out_edits: Vec::new(),
            transport: false,
            moved: false,
            history,
        };
        if let Some((app, path)) = history
            && sync(app, path, state, &mut mem)
        {
            pass.out_edits.push(Edit::Restored);
        }
        // The agents' commands since the last pass.
        let registry = id.as_ref().map(|_| plugin::Timelines::of(&ctx));
        if let (Some(reg), Some(id)) = (&registry, &id) {
            for (op, origin) in reg.drawn(id, state) {
                pass.run(&ctx, state, &mut mem, op, &origin);
            }
        }

        let width = width.unwrap_or_else(|| ui.available_width()).max(40.0);
        let lanes_h = lanes.unwrap_or(tk.ruler_height);
        let size = vec2(width, tk.ruler_height + tk.lane_height + lanes_h);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let ruler_rect = Rect::from_min_size(rect.min, vec2(width, tk.ruler_height));
        let lane = Rect::from_min_size(
            pos2(rect.left(), ruler_rect.bottom()),
            vec2(width, tk.lane_height),
        );
        let lanes_rect = Rect::from_min_max(pos2(rect.left(), lane.bottom()), rect.max);
        let font = FontId::monospace(tk.font_size);
        let painter = ui.painter_at(rect);
        let measure = |s: &str| {
            painter
                .layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        };

        state.view = state.fit_view(state.view);
        let view_before = state.view;
        let enabled = ui.is_enabled();
        let (hover, mods, zoom, scroll, press, primary_pressed, middle_down) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.modifiers,
                i.zoom_delta(),
                i.smooth_scroll_delta,
                i.pointer.press_origin(),
                i.pointer.primary_pressed(),
                i.pointer.button_down(PointerButton::Middle),
            )
        });
        let time_at = |view: &TimeView, x: f32| view.time_at(x - rect.left(), width);

        // Zoom and pan by the wheel and pinch.
        if enabled && response.hovered() {
            if zoom != 1.0 {
                let at = hover.map_or(state.playhead, |p| time_at(&state.view, p.x));
                state.view = state.fit_view(state.view.zoomed(f64::from(zoom), at));
            } else if scroll.x != 0.0 || scroll.y != 0.0 {
                let per_point = state.view.duration / f64::from(width);
                state.view.start -= f64::from(scroll.x + scroll.y) * per_point;
                state.view = state.fit_view(state.view);
                ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
            }
        }

        let mut ticks = ruler::ticks(&state.scale, &state.view, width, &measure);
        let snapper =
            |state: &TimelineState, ticks: Ticks, t: f64, skip: Option<u64>, scrub: bool| {
                snap_time(state, &snap, ticks, width, t, skip, scrub, mods.alt)
            };

        // The pointer on the ruler and the lanes.
        let mut scrubbing = false;
        if enabled {
            if take_focus {
                response.request_focus();
            }
            let pressed_here = primary_pressed && response.is_pointer_button_down_on();
            if pressed_here && !mods.shift && !mods.command && !mods.ctrl {
                response.request_focus();
                if let Some(p) = hover {
                    let t = snapper(state, ticks, time_at(&state.view, p.x), None, true);
                    pass.run(&ctx, state, &mut mem, Op::SetTime(t), &Origin::User);
                }
            } else if pressed_here {
                response.request_focus();
            }
            if response.drag_started() {
                let origin_x = press.map_or(rect.center().x, |p| p.x);
                mem.drag = if middle_down || mods.command || mods.ctrl {
                    Drag::Pan {
                        start: state.view.start,
                        x: origin_x,
                    }
                } else if mods.shift {
                    Drag::Range {
                        anchor: snapper(state, ticks, time_at(&state.view, origin_x), None, false),
                    }
                } else {
                    Drag::Scrub
                };
            }
            if response.dragged()
                && let Some(p) = hover
            {
                match mem.drag {
                    Drag::Pan { start, x } => {
                        let per_point = state.view.duration / f64::from(width);
                        state.view.start = start - f64::from(p.x - x) * per_point;
                        state.view = state.fit_view(state.view);
                    }
                    Drag::Range { anchor } => {
                        let t = snapper(state, ticks, time_at(&state.view, p.x), None, false);
                        mem.preview = Some(TimeRange::new(anchor, t));
                    }
                    Drag::Scrub => {
                        scrubbing = true;
                        let t = snapper(state, ticks, time_at(&state.view, p.x), None, true);
                        pass.run(&ctx, state, &mut mem, Op::SetTime(t), &Origin::User);
                    }
                    Drag::None | Drag::Marker { .. } => {}
                }
            }
            if response.drag_stopped() {
                if let (Drag::Range { .. }, Some(r)) = (mem.drag, mem.preview.take())
                    && r.duration() > 0.0
                {
                    pass.run(&ctx, state, &mut mem, Op::SetRange(Some(r)), &Origin::User);
                }
                mem.drag = Drag::None;
                mem.preview = None;
            }
            if response.double_clicked()
                && let Some(p) = hover
                && lane.contains(p)
            {
                let t = snapper(state, ticks, time_at(&state.view, p.x), None, false);
                let label = new_marker_label(&ctx, state);
                pass.run(
                    &ctx,
                    state,
                    &mut mem,
                    Op::AddMarker(Marker::new(0, t, label)),
                    &Origin::User,
                );
            }
        }

        // The markers: each a flag on the markers' lane.
        let x_of = |view: &TimeView, t: f64| rect.left() + view.x_of(t, width);
        let flags: Vec<(u64, Rect, String)> = state
            .markers
            .iter()
            .filter_map(|m| {
                let x = x_of(&state.view, m.time);
                if x < rect.left() - 2.0 || x > rect.right() + 2.0 {
                    return None;
                }
                let label_w = if m.label.is_empty() {
                    0.0
                } else {
                    measure(&m.label) + 4.0
                };
                let flag = Rect::from_min_size(
                    pos2(x - 5.0, lane.top() + 1.0),
                    vec2(10.0 + label_w, lane.height() - 2.0),
                )
                .intersect(lane);
                let name = marker_name(&ctx, state, m);
                Some((m.id, flag, name))
            })
            .collect();
        for (id, flag, name) in &flags {
            if !enabled {
                break;
            }
            let r = ui.interact(*flag, key.with(("marker", id)), Sense::CLICK | Sense::DRAG);
            a11y::name(&r, Role::Button, name);
            if r.hovered() || r.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if r.clicked() || r.drag_started() {
                state.selected = Some(*id);
                ui.memory_mut(|m| m.request_focus(response.id));
            }
            if r.clicked()
                && let Some(t) = state.marker(*id).map(|m| m.time)
            {
                pass.run(&ctx, state, &mut mem, Op::SetTime(t), &Origin::User);
            }
            if r.drag_started()
                && let Some(m) = state.marker(*id)
            {
                mem.drag = Drag::Marker {
                    id: *id,
                    from: m.time,
                };
            }
            if r.dragged()
                && let Some(p) = hover
            {
                let t = snapper(state, ticks, time_at(&state.view, p.x), Some(*id), false);
                state.move_marker(*id, t);
            }
            if r.drag_stopped() {
                if let Drag::Marker { id, from } = mem.drag {
                    pass.run(
                        &ctx,
                        state,
                        &mut mem,
                        Op::CommitMove { id, from },
                        &Origin::User,
                    );
                }
                mem.drag = Drag::None;
            }
        }

        // The keys.
        if response.has_focus() && enabled {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: state.selected.is_some(),
                        ..Default::default()
                    },
                );
            });
            egui_pop_core::focus::takes_keys(&ctx, response.id);
            let k_held = ui.input(|i| i.key_down(Key::K));
            let selected = state.selected.is_some();
            let acts = ui.input_mut(|i| {
                let mut acts = Vec::new();
                i.events.retain(|e| match e {
                    Event::Key {
                        key,
                        pressed: true,
                        repeat,
                        modifiers,
                        ..
                    } => match key_act(*key, *modifiers, *repeat, k_held, selected) {
                        Some(a) => {
                            acts.push(a);
                            false
                        }
                        None => true,
                    },
                    _ => true,
                });
                acts
            });
            for act in acts {
                key_run(&ctx, state, &mut pass, &mut mem, act);
            }
        }
        // A screen reader's actions on the slider.
        if enabled {
            let (inc, dec, set) = ui.input(|i| {
                let set = i
                    .accesskit_action_requests(response.id, Action::SetValue)
                    .find_map(|r| match &r.data {
                        Some(ActionData::NumericValue(v)) => Some(*v),
                        Some(ActionData::Value(text)) => state.scale.parse(text),
                        _ => None,
                    });
                (
                    i.num_accesskit_action_requests(response.id, Action::Increment),
                    i.num_accesskit_action_requests(response.id, Action::Decrement),
                    set,
                )
            });
            let unit = state.scale.step(false);
            for (n, by) in [(inc, unit), (dec, -unit)] {
                for _ in 0..n {
                    pass.run(&ctx, state, &mut mem, Op::StepBy(by), &Origin::User);
                }
            }
            if let Some(t) = set {
                pass.run(&ctx, state, &mut mem, Op::SetTime(t), &Origin::User);
            }
        }

        // Playing, the view follows the playhead a page at a time.
        if follow
            && state.is_playing()
            && mem.drag == Drag::None
            && !state.view.contains(state.playhead)
        {
            let d = state.view.duration;
            let start = if state.speed >= 0.0 {
                state.playhead - d * 0.05
            } else {
                state.playhead - d * 0.95
            };
            state.view = state.fit_view(TimeView::new(start, d));
        }
        let view_changed = state.view != view_before;
        if view_changed {
            ticks = ruler::ticks(&state.scale, &state.view, width, &measure);
        }

        // Accessibility: a slider over the time.
        let name = label.unwrap_or_else(|| tr(&ctx, &txt::TIMELINE));
        a11y::name(&response, Role::Slider, &name);
        let value = state.scale.format(state.playhead);
        let (lo, hi) = state
            .bounds
            .map_or((state.view.start, state.view.end()), |b| (b.start, b.end));
        let unit = state.scale.step(false);
        let playhead = state.playhead;
        ctx.accesskit_node_builder(response.id, |node| {
            node.set_value(value);
            node.set_numeric_value(playhead);
            node.set_min_numeric_value(lo.min(playhead));
            node.set_max_numeric_value(hi.max(playhead));
            node.set_numeric_value_step(unit);
            node.add_action(Action::SetValue);
            node.add_action(Action::Increment);
            node.add_action(Action::Decrement);
        });
        egui_pop_core::focus::own_ring(&ctx, response.id);

        // Drawing.
        if ui.is_rect_visible(rect) {
            paint_base(&painter, &tk, state, &mem, rect, ruler_rect, ticks, &font);
        }
        let lanes = Lanes {
            rect: lanes_rect,
            view: state.view,
            ticks,
        };
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(lanes_rect)
                .id_salt(key.with("lanes")),
        );
        child.set_clip_rect(lanes_rect.intersect(ui.clip_rect()));
        body(&mut child, &lanes);
        if ui.is_rect_visible(rect) {
            paint_over(&painter, &tk, state, rect, lane, ruler_rect, &font, &ctx);
            if response.has_focus() {
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, tk.focus), StrokeKind::Inside);
            }
        }

        if let (Some(reg), Some(id)) = (&registry, &id) {
            reg.update(id, state, response.has_focus());
        }
        ctx.data_mut(|d| d.insert_temp(key, mem));
        TimelineOutput {
            response,
            playhead_moved: pass.moved,
            scrubbing,
            transport_changed: pass.transport,
            play: state.play,
            speed: state.speed,
            view_changed,
            edits: pass.out_edits,
        }
    }
}

/// Runs a key's act.
fn key_run(
    ctx: &egui::Context,
    state: &mut TimelineState,
    pass: &mut Pass<'_>,
    mem: &mut Memory,
    act: KeyAct,
) {
    let user = &Origin::User;
    let mut run = |state: &mut TimelineState, mem: &mut Memory, op: Op| {
        pass.run(ctx, state, mem, op, user);
    };
    match act {
        KeyAct::Toggle => run(state, mem, Op::Toggle),
        KeyAct::Pause => run(state, mem, Op::Pause),
        KeyAct::Shuttle(forward) => run(state, mem, Op::Shuttle(forward)),
        KeyAct::Step(sign, big) => {
            let by = sign * state.scale.step(big);
            run(state, mem, Op::StepBy(by));
        }
        KeyAct::Home => {
            let t = state.home();
            run(state, mem, Op::SetTime(t));
        }
        KeyAct::End => {
            let t = state.end();
            run(state, mem, Op::SetTime(t));
        }
        KeyAct::In => {
            let r = state.in_at(state.playhead);
            run(state, mem, Op::SetRange(Some(r)));
        }
        KeyAct::Out => {
            let r = state.out_at(state.playhead);
            run(state, mem, Op::SetRange(Some(r)));
        }
        KeyAct::GoIn | KeyAct::GoOut => {
            if let Some(r) = state.range {
                let t = if act == KeyAct::GoIn { r.start } else { r.end };
                run(state, mem, Op::SetTime(t));
            }
        }
        KeyAct::ClearRange => run(state, mem, Op::SetRange(None)),
        KeyAct::ToggleLoop => {
            let on = !state.looping;
            run(state, mem, Op::SetLoop(on));
        }
        KeyAct::AddMarker => {
            let label = new_marker_label(ctx, state);
            let m = Marker::new(0, state.playhead, label);
            run(state, mem, Op::AddMarker(m));
        }
        KeyAct::Neighbour(forward) => {
            if let Some(id) = state.neighbour_marker(forward) {
                state.selected = Some(id);
                if let Some(m) = state.marker(id).cloned() {
                    run(state, mem, Op::SetTime(m.time));
                    a11y::announce(ctx, marker_name(ctx, state, &m), Politeness::Polite);
                }
            }
        }
        KeyAct::Nudge(sign, big) => {
            if let Some(m) = state.selected.and_then(|id| state.marker(id)).cloned() {
                let to = m.time + sign * state.scale.step(big);
                run(state, mem, Op::MoveMarker(m.id, to));
            }
        }
        KeyAct::Remove => {
            if let Some(id) = state.selected {
                run(state, mem, Op::RemoveMarker(id));
            }
        }
        KeyAct::Deselect => state.selected = None,
        KeyAct::Zoom(factor) => {
            state.view = state.fit_view(state.view.zoomed(factor, state.playhead));
        }
    }
}

/// What a key asks.
#[derive(Clone, Copy, Debug, PartialEq)]
enum KeyAct {
    Toggle,
    Pause,
    Shuttle(bool),
    /// -1 or 1, and whether the big step.
    Step(f64, bool),
    Home,
    End,
    In,
    Out,
    GoIn,
    GoOut,
    ClearRange,
    ToggleLoop,
    AddMarker,
    Neighbour(bool),
    Nudge(f64, bool),
    Remove,
    Deselect,
    Zoom(f64),
}

/// The act of a key pressed with exactly these modifiers.
fn key_act(key: Key, m: Modifiers, repeat: bool, k_held: bool, selected: bool) -> Option<KeyAct> {
    let ctrl = m.command || m.ctrl;
    let (none, shift, ctrl_only, alt_only, ctrl_shift) = (
        !ctrl && !m.shift && !m.alt,
        m.shift && !ctrl && !m.alt,
        ctrl && !m.shift && !m.alt,
        m.alt && !ctrl && !m.shift,
        ctrl && m.shift && !m.alt,
    );
    let once = !repeat;
    Some(match key {
        Key::Space if none && once => KeyAct::Toggle,
        Key::K if none && once => KeyAct::Pause,
        Key::J if none && k_held => KeyAct::Step(-1.0, false),
        Key::L if none && k_held => KeyAct::Step(1.0, false),
        Key::J if none && once => KeyAct::Shuttle(false),
        Key::L if none && once => KeyAct::Shuttle(true),
        Key::L if ctrl_only && once => KeyAct::ToggleLoop,
        Key::ArrowLeft if none || shift => KeyAct::Step(-1.0, shift),
        Key::ArrowRight if none || shift => KeyAct::Step(1.0, shift),
        Key::ArrowLeft if ctrl_only || ctrl_shift => KeyAct::Nudge(-1.0, ctrl_shift),
        Key::ArrowRight if ctrl_only || ctrl_shift => KeyAct::Nudge(1.0, ctrl_shift),
        Key::ArrowUp if none => KeyAct::Neighbour(false),
        Key::ArrowDown if none => KeyAct::Neighbour(true),
        Key::Home if none => KeyAct::Home,
        Key::End if none => KeyAct::End,
        Key::I if none && once => KeyAct::In,
        Key::O if none && once => KeyAct::Out,
        Key::I if shift && once => KeyAct::GoIn,
        Key::O if shift && once => KeyAct::GoOut,
        Key::X if alt_only && once => KeyAct::ClearRange,
        Key::M if none && once => KeyAct::AddMarker,
        Key::Delete | Key::Backspace if none && once && selected => KeyAct::Remove,
        Key::Escape if none && selected => KeyAct::Deselect,
        Key::Plus | Key::Equals if none || shift => KeyAct::Zoom(2.0),
        Key::Minus if none => KeyAct::Zoom(0.5),
        _ => return None,
    })
}

/// `t` snapped as [`Snap`] says (nothing with Alt held, `free`).
#[allow(clippy::too_many_arguments)]
fn snap_time(
    state: &TimelineState,
    snap: &Snap,
    ticks: Ticks,
    width: f32,
    t: f64,
    skip_marker: Option<u64>,
    scrubbing: bool,
    free: bool,
) -> f64 {
    let t = state.clamp(t);
    if free {
        return t;
    }
    let mut targets = ruler::SnapTargets {
        grid: snap.grid.then_some(ticks.minor),
        points: Vec::new(),
    };
    if snap.markers {
        targets.points.extend(
            state
                .markers
                .iter()
                .filter(|m| Some(m.id) != skip_marker)
                .map(|m| (m.time, Snapped::Marker)),
        );
        if let Some(r) = state.range {
            targets.points.push((r.start, Snapped::Range));
            targets.points.push((r.end, Snapped::Range));
        }
    }
    if snap.playhead && !scrubbing {
        targets.points.push((state.playhead, Snapped::Playhead));
    }
    let reach = f64::from(SNAP_REACH) * state.view.duration / f64::from(width.max(1.0));
    state.clamp(ruler::snap(&state.scale, t, &targets, reach).0)
}

/// "Marker 3", the next one's default label.
fn new_marker_label(ctx: &egui::Context, state: &TimelineState) -> String {
    let n = state.markers.len() + 1;
    tr_args(
        ctx,
        &txt::TIMELINE_NEW_MARKER,
        &[("number", &n.to_string())],
    )
}

/// A marker's accessible name: "Marker Drop at 1.1.000".
fn marker_name(ctx: &egui::Context, state: &TimelineState, m: &Marker) -> String {
    let key = match m.kind {
        MarkerKind::Marker => &txt::TIMELINE_MARKER,
        MarkerKind::Cue => &txt::TIMELINE_CUE,
    };
    tr_args(
        ctx,
        key,
        &[("label", &m.label), ("time", &state.scale.format(m.time))],
    )
}

/// The store's document into `state` when it changed there (an undo):
/// whether it did. Declares the path, and seeds it with `state`'s document
/// the first time.
fn sync(app: &AppHandle, path: &str, state: &mut TimelineState, mem: &mut Memory) -> bool {
    let store = app.store();
    let mut s = lock(&store);
    if s.get(path).is_none()
        && s.declare(&OwnerId::FRAMEWORK, path, Descriptor::text("Timeline", ""))
            .is_err()
    {
        return false;
    }
    let text = s
        .get(path)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .unwrap_or_default();
    if text.is_empty() {
        let json = serde_json::to_string(&state.doc()).unwrap_or_default();
        let _ = s.set(
            Set::new(path, Change::To(Value::Text(json.clone())), Origin::User)
                .untracked()
                .force(),
        );
        mem.text = Some(json);
        return false;
    }
    if mem.text.as_deref() == Some(text.as_str()) {
        return false;
    }
    let Ok(doc) = serde_json::from_str::<TimelineDoc>(&text) else {
        return false;
    };
    mem.text = Some(text);
    if doc == state.doc() {
        return false;
    }
    state.set_doc(doc);
    true
}

/// Writes `state`'s document to the store as one step named `label`.
fn commit(
    app: &AppHandle,
    path: &str,
    state: &TimelineState,
    label: &str,
    origin: &Origin,
    mem: &mut Memory,
) {
    let json = serde_json::to_string(&state.doc()).unwrap_or_default();
    let store = app.store();
    let mut s = lock(&store);
    if s.get(path).is_none() {
        let _ = s.declare(&OwnerId::FRAMEWORK, path, Descriptor::text("Timeline", ""));
    }
    let group = s.begin_group(origin.clone(), label);
    let _ = s.set(
        Set::new(path, Change::To(Value::Text(json.clone())), origin.clone())
            .gesture(group)
            .force(),
    );
    s.end(group);
    mem.text = Some(json);
}

/// The ruler, the range and the grid, under the lanes.
#[allow(clippy::too_many_arguments)]
fn paint_base(
    p: &egui::Painter,
    tk: &TimelineTokens,
    state: &TimelineState,
    mem: &Memory,
    rect: Rect,
    ruler: Rect,
    ticks: Ticks,
    font: &FontId,
) {
    let width = rect.width();
    p.rect_filled(rect, 0.0, tk.background);
    p.rect_filled(ruler, 0.0, tk.ruler_fill);
    let x_of = |t: f64| rect.left() + state.view.x_of(t, width);
    // The range: a fill down the lanes, a band under the ruler's ticks.
    if let Some(r) = mem.preview.or(state.range) {
        let (a, b) = (x_of(r.start), x_of(r.end));
        let span = Rect::from_x_y_ranges(a..=b, ruler.bottom()..=rect.bottom()).intersect(rect);
        p.rect_filled(span, 0.0, tk.range_fill);
        let edge = if state.looping {
            tk.range_edge
        } else {
            tk.range_edge_idle
        };
        let band =
            Rect::from_x_y_ranges(a..=b, ruler.bottom() - 3.0..=ruler.bottom()).intersect(rect);
        p.rect_filled(band, 0.0, edge);
        for x in [a, b] {
            if x >= rect.left() && x <= rect.right() {
                p.line_segment(
                    [pos2(x, ruler.bottom()), pos2(x, rect.bottom())],
                    Stroke::new(1.0, edge),
                );
            }
        }
    }
    let marks = ruler::marks(&state.scale, &state.view, width, ticks);
    let mut last_end = f32::NEG_INFINITY;
    for m in &marks {
        let x = rect.left() + m.x;
        if x < rect.left() - 1.0 || x > rect.right() + 1.0 {
            continue;
        }
        let h = if m.major { 0.45 } else { 0.2 } * ruler.height();
        p.line_segment(
            [pos2(x, ruler.bottom() - h), pos2(x, ruler.bottom())],
            Stroke::new(1.0, tk.ruler_tick),
        );
        if m.major {
            p.line_segment(
                [pos2(x, ruler.bottom()), pos2(x, rect.bottom())],
                Stroke::new(1.0, tk.grid),
            );
        }
    }
    // The labels, never over one another.
    for m in marks.iter().filter(|m| m.major) {
        let x = rect.left() + m.x + ruler::LABEL_INSET;
        let Some(label) = &m.label else { continue };
        let galley = p.layout_no_wrap(label.clone(), font.clone(), tk.ruler_text);
        let w = galley.size().x;
        if x + w < rect.left() || x > rect.right() || x < last_end {
            continue;
        }
        last_end = x + w + ruler::LABEL_GAP / 2.0;
        p.galley(pos2(x, ruler.top() + 2.0), galley, tk.ruler_text);
    }
    p.line_segment(
        [ruler.left_bottom(), ruler.right_bottom()],
        Stroke::new(1.0, tk.ruler_border),
    );
}

/// The markers and the playhead, over the lanes.
#[allow(clippy::too_many_arguments)]
fn paint_over(
    p: &egui::Painter,
    tk: &TimelineTokens,
    state: &TimelineState,
    rect: Rect,
    lane: Rect,
    ruler: Rect,
    font: &FontId,
    ctx: &egui::Context,
) {
    let width = rect.width();
    let x_of = |t: f64| rect.left() + state.view.x_of(t, width);
    for m in &state.markers {
        let x = x_of(m.time);
        if x < rect.left() - 1.0 || x > rect.right() + 1.0 {
            continue;
        }
        let color = m.color.unwrap_or(tk.marker);
        p.line_segment(
            [pos2(x, lane.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, color.gamma_multiply(0.7)),
        );
        let top = lane.top() + 2.0;
        let head = match m.kind {
            // A flag on its pole.
            MarkerKind::Marker => {
                let flag = Rect::from_min_size(pos2(x, top), vec2(7.0, 7.0));
                p.rect_filled(flag, 1.0, color);
                flag
            }
            // A pin pointing down at its time.
            MarkerKind::Cue => {
                let pin = vec![pos2(x - 5.0, top), pos2(x + 5.0, top), pos2(x, top + 8.0)];
                let bounds = Rect::from_min_max(pos2(x - 5.0, top), pos2(x + 5.0, top + 8.0));
                p.add(Shape::convex_polygon(pin, color, Stroke::NONE));
                bounds
            }
        };
        let mut around = head;
        if !m.label.is_empty() {
            let galley = p.layout_no_wrap(m.label.clone(), font.clone(), tk.marker_text);
            let at = pos2(head.right() + 3.0, lane.center().y - galley.size().y / 2.0);
            around = around.union(Rect::from_min_size(at, galley.size()));
            p.galley(at, galley, tk.marker_text);
        }
        if state.selected == Some(m.id) {
            p.rect_stroke(
                around.expand(2.0),
                tk.radius.min(3.0),
                Stroke::new(1.0, tk.marker_selected),
                StrokeKind::Outside,
            );
        }
    }
    // The playhead: a head on the ruler, a line down the lanes.
    let x = x_of(state.playhead);
    if x >= rect.left() - 6.0 && x <= rect.right() + 6.0 {
        p.line_segment(
            [pos2(x, ruler.top()), pos2(x, rect.bottom())],
            Stroke::new(1.5, tk.playhead),
        );
        let head: Vec<Pos2> = vec![
            pos2(x - 6.0, ruler.top()),
            pos2(x + 6.0, ruler.top()),
            pos2(x + 6.0, ruler.top() + 5.0),
            pos2(x, ruler.top() + 11.0),
            pos2(x - 6.0, ruler.top() + 5.0),
        ];
        p.add(Shape::convex_polygon(head, tk.playhead, Stroke::NONE));
        // Shuttling: the speed beside the head.
        if state.is_playing() && state.speed != 1.0 {
            let speed = format_speed(state.speed);
            let text = tr_args(ctx, &txt::TIMELINE_SPEED, &[("speed", &speed)]);
            let galley = p.layout_no_wrap(text, font.clone(), tk.badge_text);
            let size = galley.size() + vec2(6.0, 2.0);
            let at = if x + 8.0 + size.x <= rect.right() {
                pos2(x + 8.0, ruler.top() + 1.0)
            } else {
                pos2(x - 8.0 - size.x, ruler.top() + 1.0)
            };
            let badge = Rect::from_min_size(at, size);
            p.rect_filled(badge, tk.radius.min(3.0), tk.badge_fill);
            p.galley(badge.center() - galley.size() / 2.0, galley, tk.badge_text);
        }
    }
}

/// `2`, `-4`, `0.5`.
fn format_speed(speed: f64) -> String {
    if speed.fract() == 0.0 {
        format!("{}", speed as i64)
    } else {
        format!("{speed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_with_their_exact_modifiers() {
        let none = Modifiers::NONE;
        let shift = Modifiers::SHIFT;
        let ctrl = Modifiers::COMMAND;
        let alt = Modifiers::ALT;
        let act = |k, m| key_act(k, m, false, false, true);
        assert_eq!(act(Key::Space, none), Some(KeyAct::Toggle));
        assert_eq!(act(Key::L, none), Some(KeyAct::Shuttle(true)));
        assert_eq!(act(Key::L, ctrl), Some(KeyAct::ToggleLoop));
        assert_eq!(act(Key::ArrowLeft, shift), Some(KeyAct::Step(-1.0, true)));
        assert_eq!(act(Key::ArrowRight, ctrl), Some(KeyAct::Nudge(1.0, false)));
        assert_eq!(act(Key::I, shift), Some(KeyAct::GoIn));
        assert_eq!(act(Key::X, alt), Some(KeyAct::ClearRange));
        assert_eq!(act(Key::X, none), None);
        assert_eq!(act(Key::M, ctrl), None);
        // K held: J and L step.
        assert_eq!(
            key_act(Key::L, none, false, true, false),
            Some(KeyAct::Step(1.0, false))
        );
        // A held Space does not toggle again and again.
        assert_eq!(key_act(Key::Space, none, true, false, false), None);
        // Delete and Escape only with a marker selected.
        assert_eq!(key_act(Key::Delete, none, false, false, false), None);
        assert_eq!(key_act(Key::Escape, none, false, false, false), None);
    }

    #[test]
    fn ops_report_edits_and_transport() {
        let mut s = TimelineState::new(TimeScale::Seconds, 60.0);
        let done = apply(&mut s, Op::Shuttle(true));
        assert!(done.transport && done.edit.is_none());
        let done = apply(&mut s, Op::SetRange(Some(TimeRange::new(5.0, 2.0))));
        assert_eq!(
            done.edit,
            Some(Edit::RangeSet {
                from: None,
                to: Some(TimeRange::new(2.0, 5.0))
            })
        );
        // The same again changes nothing.
        let done = apply(&mut s, Op::SetRange(Some(TimeRange::new(2.0, 5.0))));
        assert!(done.edit.is_none());
        let done = apply(&mut s, Op::AddMarker(Marker::new(0, 3.0, "A")));
        assert!(matches!(done.edit, Some(Edit::MarkerAdded(_))));
        assert_eq!(s.selected, Some(1));
        let done = apply(&mut s, Op::SetTime(7.0));
        assert!(done.moved && done.edit.is_none());
        assert_eq!(format_speed(-4.0), "-4");
        assert_eq!(format_speed(0.5), "0.5");
    }
}
