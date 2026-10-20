//! A curve editor: control points the user adds, moves and deletes,
//! through a spline the application supplies ([`Spline`]), over an
//! optional histogram, with the targeted adjustment of photo editors (a
//! value picked under the pointer in the image selects or adds the
//! point to drag).
//!
//! Points are `[x, y]` in 0..1, sorted by `x`, at least two. The editor
//! owns no curve maths: [`Spline::eval`] draws the curve through them
//! ([`MonotoneCubic`] by default, [`Linear`] for straight segments).
//!
//! Gestures:
//!
//! - **On a point**: a press selects it, a drag moves it between its
//!   neighbours (Ctrl finer, Shift coarser; Escape puts it back), a
//!   double click or a right click deletes it (two points stay).
//! - **On the curve**: a press adds a point there, and a drag moves it
//!   at once.
//! - **Elsewhere**: a double click puts the curve back to its default
//!   ([`CurveEditor::default_points`], the identity unless given).
//! - **Keys** when focused ([`CurveKeys`]): Page Down and Page Up select
//!   the next and the previous point, the arrows move the selected one
//!   (Ctrl finer, Shift coarser), Insert adds a point halfway to the
//!   next one, Delete or Backspace deletes it, R resets the curve.
//! - **Pick** ([`CurveEditor::pick`]): the application passes the value
//!   under the pointer in the image; the editor marks it and highlights
//!   the nearest point; pressing selects that point, or adds one on the
//!   curve there, and the application's vertical drag moves it.
//!
//! Every change goes through the [`on_change`](CurveEditor::on_change)
//! hook ([`Change`] of the whole list of points): a drag's frames with
//! `done: false`, its end once more from the points before it with
//! `done: true` (a veto puts them back), and each key, addition,
//! deletion and reset. `egui-pop-imaging` runs the finished ones as the
//! `curve.change` step of an application.
//!
//! **Accessibility.** The editor is an AccessKit slider named by its
//! label, whose value says which point is selected and where; increment
//! and decrement move that point's output by a step, set-value sets it.
//!
//! ```
//! use egui_pop_viewport::curve::{CurveEditor, Pick, PickAction};
//!
//! fn tone(ui: &mut egui::Ui, points: &mut Vec<[f32; 2]>, luma_bins: &[f32], under: Option<f32>) {
//!     CurveEditor::new(points, "Tone curve")
//!         .histogram(luma_bins)
//!         .pick(under.map(|x| Pick { x, action: PickAction::Hover }))
//!         .show(ui);
//! }
//! ```

use egui::{
    Color32, Id, Key, Modifiers, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
    Widget, WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;

use crate::change::{self, Cause, Change, Gesture, Hook, Verdict};

/// The curve's maths, supplied by the application.
pub trait Spline {
    /// The curve's output at `x` (0..1), through `points` (sorted by
    /// `x`, at least two).
    fn eval(&self, points: &[[f32; 2]], x: f32) -> f32;
}

/// Straight segments between the points, flat past the ends.
#[derive(Clone, Copy, Debug, Default)]
pub struct Linear;

impl Spline for Linear {
    fn eval(&self, points: &[[f32; 2]], x: f32) -> f32 {
        let Some(i) = segment(points, x) else {
            return ends(points, x);
        };
        let ([x0, y0], [x1, y1]) = (points[i], points[i + 1]);
        let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 0.0 };
        y0 + (y1 - y0) * t
    }
}

/// A monotone cubic (Fritsch and Carlson): smooth, through every
/// point, and never overshooting between two of them, so a rising set
/// of points gives a rising curve. Flat past the ends.
#[derive(Clone, Copy, Debug, Default)]
pub struct MonotoneCubic;

impl Spline for MonotoneCubic {
    fn eval(&self, points: &[[f32; 2]], x: f32) -> f32 {
        let Some(i) = segment(points, x) else {
            return ends(points, x);
        };
        let n = points.len();
        let slope = |k: usize| {
            let ([x0, y0], [x1, y1]) = (points[k], points[k + 1]);
            if x1 > x0 { (y1 - y0) / (x1 - x0) } else { 0.0 }
        };
        // The tangent at point k: the mean of the slopes around it, set
        // to zero at a peak or a valley and limited so the segment stays
        // monotone.
        let tangent = |k: usize| {
            if k == 0 {
                return slope(0);
            }
            if k == n - 1 {
                return slope(n - 2);
            }
            let (a, b) = (slope(k - 1), slope(k));
            if a * b <= 0.0 {
                0.0
            } else {
                // The harmonic mean keeps it within 3 times either slope.
                2.0 * a * b / (a + b)
            }
        };
        let ([x0, y0], [x1, y1]) = (points[i], points[i + 1]);
        let h = x1 - x0;
        if h <= 0.0 {
            return y0;
        }
        let s = slope(i);
        let (mut m0, mut m1) = (tangent(i), tangent(i + 1));
        if s == 0.0 {
            m0 = 0.0;
            m1 = 0.0;
        } else {
            let (a, b) = (m0 / s, m1 / s);
            let r = a * a + b * b;
            if r > 9.0 {
                let t = 3.0 / r.sqrt();
                m0 = t * a * s;
                m1 = t * b * s;
            }
        }
        let t = (x - x0) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * h * m0
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * h * m1
    }
}

/// The segment `i..i+1` holding `x`, or `None` past the ends.
fn segment(points: &[[f32; 2]], x: f32) -> Option<usize> {
    if points.len() < 2 || x < points[0][0] || x > points[points.len() - 1][0] {
        return None;
    }
    Some(
        points
            .windows(2)
            .position(|w| x <= w[1][0])
            .unwrap_or(points.len() - 2),
    )
}

/// The value past the ends: the end's own.
fn ends(points: &[[f32; 2]], x: f32) -> f32 {
    match points {
        [] => x,
        [p] => p[1],
        [first, .., last] => {
            if x <= first[0] {
                first[1]
            } else {
                last[1]
            }
        }
    }
}

/// The identity: output equals input.
pub fn identity() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [1.0, 1.0]]
}

/// The smallest distance kept between two points' inputs.
pub const MIN_GAP: f32 = 0.01;

/// The index of the point whose input is nearest `x`, within
/// `tolerance`.
pub fn nearest(points: &[[f32; 2]], x: f32, tolerance: f32) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| (i, (p[0] - x).abs()))
        .filter(|(_, d)| *d <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Inserts a point in order; its index, or `None` when it is too close
/// to another point's input.
pub fn insert(points: &mut Vec<[f32; 2]>, p: [f32; 2]) -> Option<usize> {
    let p = [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0)];
    if points.iter().any(|q| (q[0] - p[0]).abs() < MIN_GAP) {
        return None;
    }
    let i = points
        .iter()
        .position(|q| q[0] > p[0])
        .unwrap_or(points.len());
    points.insert(i, p);
    Some(i)
}

/// Point `i` moved to `to`, kept inside 0..1 and between its
/// neighbours' inputs.
pub fn moved(points: &[[f32; 2]], i: usize, to: [f32; 2]) -> [f32; 2] {
    let lo = if i == 0 {
        0.0
    } else {
        points[i - 1][0] + MIN_GAP
    };
    let hi = if i + 1 >= points.len() {
        1.0
    } else {
        points[i + 1][0] - MIN_GAP
    };
    let x = if lo <= hi {
        to[0].clamp(lo, hi)
    } else {
        points[i][0]
    };
    [x, to[1].clamp(0.0, 1.0)]
}

/// What the application does with the value picked in the image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PickAction {
    /// The pointer is over the image: mark the value, highlight the
    /// nearest point.
    Hover,
    /// The pointer pressed: select the nearest point, or add one on the
    /// curve; the gesture begins.
    Press,
    /// The pointer moved during the gesture: move the selected point's
    /// output by this much (0..1 units, up positive).
    Move(f32),
    /// The gesture ended.
    Release,
    /// The gesture was given up: the points go back.
    Cancel,
}

/// A value picked under the pointer in the image (its input on the
/// curve, 0..1) and what the pointer does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    pub x: f32,
    pub action: PickAction,
}

/// The editor's keys, as data so a profile can remap them
/// ([`set_keys`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveKeys {
    pub next: Key,
    pub previous: Key,
    pub add: Key,
    pub delete: [Key; 2],
    pub reset: Key,
}

impl Default for CurveKeys {
    fn default() -> Self {
        CurveKeys {
            next: Key::PageDown,
            previous: Key::PageUp,
            add: Key::Insert,
            delete: [Key::Delete, Key::Backspace],
            reset: Key::R,
        }
    }
}

const KEYS: Slot<CurveKeys> = Slot::new("viewport.curve.keys");
/// An editor's selection and drag, per editor.
const MEMORY: Slot<Memory> = Slot::new("viewport.curve.memory");

/// The curve editors' keys in a context (the defaults until set).
pub fn keys(ctx: &egui::Context) -> CurveKeys {
    KEYS.get_or_default(ctx)
}

/// Remaps the curve editors' keys in a context (an input profile).
pub fn set_keys(ctx: &egui::Context, keys: CurveKeys) {
    KEYS.set(ctx, keys);
}

/// The curve editor's colours, from the theme (`curve.*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveTokens {
    pub background: Color32,
    pub grid: Color32,
    pub identity: Color32,
    pub line: Color32,
    pub point: Color32,
    pub point_selected: Color32,
    pub pick: Color32,
}

impl CurveTokens {
    pub fn of(ctx: &egui::Context) -> CurveTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        CurveTokens {
            background: c("curve.background", t.surface.sunken),
            grid: c("curve.grid", t.border.default),
            identity: c("curve.identity", t.border.default),
            line: c("curve.line", t.text.default),
            point: c("curve.point", t.text.default),
            point_selected: c("curve.point-selected", t.accent.default),
            pick: c("curve.pick", t.status.warning),
        }
    }
}

type Points = Vec<[f32; 2]>;

static MONOTONE: MonotoneCubic = MonotoneCubic;

/// The curve editor.
#[derive(Debug)]
pub struct CurveEditor<'a> {
    points: &'a mut Points,
    label: String,
    spline: &'a dyn Spline,
    defaults: Option<Points>,
    histogram: Option<&'a [f32]>,
    pick: Option<Pick>,
    size: Option<Vec2>,
    color: Option<Color32>,
    step: f32,
    max_points: usize,
    hook: Option<Hook<'a, Points>>,
}

impl std::fmt::Debug for dyn Spline + '_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Spline")
    }
}

/// What the editor did in a frame.
#[derive(Debug)]
pub struct CurveOutput {
    /// The editor; `changed()` when the points changed.
    pub response: Response,
    /// The changes written this frame, after the hook.
    pub changes: Vec<Change<Points>>,
    /// The point selected after this frame.
    pub selected: Option<usize>,
}

/// A gesture under way: the points before it, the point moved, where it
/// was and how far the pointer took it.
#[derive(Clone, Debug)]
struct Grab {
    start: Points,
    index: usize,
    origin: [f32; 2],
    moved: [f32; 2],
    cause: Cause,
}

/// What the editor keeps between frames.
#[derive(Clone, Debug, Default)]
struct Memory {
    selected: Option<usize>,
    grab: Option<Grab>,
}

impl<'a> CurveEditor<'a> {
    /// An editor of `points`, named `label` for screen readers.
    pub fn new(points: &'a mut Vec<[f32; 2]>, label: &str) -> CurveEditor<'a> {
        CurveEditor {
            points,
            label: label.to_string(),
            spline: &MONOTONE,
            defaults: None,
            histogram: None,
            pick: None,
            size: None,
            color: None,
            step: 0.01,
            max_points: 20,
            hook: None,
        }
    }

    /// The curve's maths ([`MonotoneCubic`] by default).
    pub fn spline(mut self, spline: &'a dyn Spline) -> Self {
        self.spline = spline;
        self
    }

    /// The points a reset goes back to (the identity by default).
    pub fn default_points(mut self, points: Vec<[f32; 2]>) -> Self {
        self.defaults = Some(points);
        self
    }

    /// Bins drawn behind the curve (any scale).
    pub fn histogram(mut self, bins: &'a [f32]) -> Self {
        self.histogram = Some(bins);
        self
    }

    /// The value picked under the pointer in the image, and what the
    /// pointer does (the targeted adjustment).
    pub fn pick(mut self, pick: Option<Pick>) -> Self {
        self.pick = pick;
        self
    }

    /// The size in points (default: the available width, square, at
    /// most 320).
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// The curve's colour (a channel's; `curve.line` by default).
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// How far an arrow key moves a point (a hundredth by default).
    pub fn step(mut self, step: f32) -> Self {
        self.step = step.abs();
        self
    }

    /// The most points the curve may have (20 by default).
    pub fn max_points(mut self, n: usize) -> Self {
        self.max_points = n.max(2);
        self
    }

    /// Sees each change before it is written: allow, veto or redirect
    /// (other points).
    pub fn on_change(mut self, hook: impl FnMut(&Change<Points>) -> Verdict<Points> + 'a) -> Self {
        self.hook = Some(Hook::new(hook));
        self
    }

    /// Draws the editor; says what changed.
    pub fn show(self, ui: &mut Ui) -> CurveOutput {
        let CurveEditor {
            points,
            label,
            spline,
            defaults,
            histogram,
            pick,
            size,
            color,
            step,
            max_points,
            mut hook,
        } = self;
        let size = size.unwrap_or_else(|| {
            let w = ui.available_width().clamp(120.0, 320.0);
            vec2(w, w)
        });
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let id = response.id;
        let area = rect.shrink(6.0);
        let mem_id = MEMORY.keyed(id);
        let mut mem = mem_id.get_or_default(ui.ctx());
        if mem.selected.is_some_and(|i| i >= points.len()) {
            mem.selected = None;
        }
        let defaults = defaults.unwrap_or_else(identity);
        let mut ed = Edit {
            points,
            hook: &mut hook,
            changes: Vec::new(),
        };
        if ed.points.len() >= 2 {
            let map = Map { area };
            pointer(
                ui, &response, &map, spline, &defaults, max_points, &mut mem, &mut ed,
            );
            if let Some(p) = pick {
                picked(p, spline, max_points, &mut mem, &mut ed);
            }
            if response.has_focus() {
                change::lock_arrows(ui, id);
                keyboard(ui, spline, &defaults, step, max_points, &mut mem, &mut ed);
            }
            access(ui, id, step, &mut mem, &mut ed);
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        if !ed.changes.is_empty() {
            response.mark_changed();
        }
        let changes = ed.changes;
        let points: &[[f32; 2]] = &ed.points[..];
        paint(
            ui, rect, area, &response, points, spline, histogram, pick, color, &mem,
        );
        let value = value_text(ui.ctx(), points, mem.selected);
        let enabled = ui.is_enabled();
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(WidgetType::Slider, enabled, &label);
            info.current_text_value = Some(value.clone());
            info
        });
        let selected = mem.selected;
        let out_value = selected.and_then(|i| points.get(i)).map(|p| p[1]);
        ui.ctx().accesskit_node_builder(id, |node| {
            use egui::accesskit::Action;
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value(1.0);
            node.set_numeric_value_step(f64::from(step));
            if let Some(v) = out_value {
                node.set_numeric_value(f64::from(v));
                node.add_action(Action::Increment);
                node.add_action(Action::Decrement);
                node.add_action(Action::SetValue);
            }
        });
        mem_id.set(ui.ctx(), mem);
        CurveOutput {
            response,
            changes,
            selected,
        }
    }
}

impl Widget for CurveEditor<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

/// The points being edited, the hook, and what was written.
struct Edit<'e, 'a> {
    points: &'e mut Points,
    hook: &'e mut Option<Hook<'a, Points>>,
    changes: Vec<Change<Points>>,
}

impl Edit<'_, '_> {
    /// Proposes `to`; writes what the hook leaves. Whether written.
    fn propose(&mut self, to: Points, cause: Cause, done: bool) -> bool {
        if to == *self.points {
            return false;
        }
        let change = Change {
            from: self.points.clone(),
            to,
            cause,
            done,
        };
        match change::decide(self.hook, change) {
            Some(c) => {
                self.points.clone_from(&c.to);
                self.changes.push(c);
                true
            }
            None => false,
        }
    }

    /// A gesture's end: one done change from the points before it; a
    /// veto puts them back.
    fn finish(&mut self, grab: Grab) {
        if grab.start == *self.points {
            return;
        }
        let change = Change {
            from: grab.start.clone(),
            to: self.points.clone(),
            cause: grab.cause,
            done: true,
        };
        match change::decide(self.hook, change) {
            Some(c) => {
                self.points.clone_from(&c.to);
                self.changes.push(c);
            }
            None => self.points.clone_from(&grab.start),
        }
    }

    /// Back to the points before the gesture, with no trace.
    fn cancel(&mut self, grab: Grab) {
        self.points.clone_from(&grab.start);
    }

    /// Moves the grabbed point by the pointer's motion (frames).
    fn drag(&mut self, grab: &mut Grab, delta: [f32; 2]) {
        grab.moved = [grab.moved[0] + delta[0], grab.moved[1] + delta[1]];
        if grab.index >= self.points.len() {
            return;
        }
        let want = [
            grab.origin[0] + grab.moved[0],
            grab.origin[1] + grab.moved[1],
        ];
        let mut to = self.points.clone();
        to[grab.index] = moved(&to, grab.index, want);
        self.propose(to, grab.cause, false);
    }
}

/// Between curve units and the screen.
struct Map {
    area: Rect,
}

impl Map {
    fn to_screen(&self, p: [f32; 2]) -> Pos2 {
        pos2(
            self.area.left() + p[0] * self.area.width(),
            self.area.bottom() - p[1] * self.area.height(),
        )
    }

    fn unit_at(&self, pos: Pos2) -> [f32; 2] {
        [
            (pos.x - self.area.left()) / self.area.width().max(1.0),
            (self.area.bottom() - pos.y) / self.area.height().max(1.0),
        ]
    }

    /// A motion in points as curve units.
    fn delta(&self, d: Vec2) -> [f32; 2] {
        [
            d.x / self.area.width().max(1.0),
            -d.y / self.area.height().max(1.0),
        ]
    }
}

/// How close, in points, a press must be to a point or the curve.
const HIT: f32 = 8.0;

/// The pointer: grab, drag, add, delete, reset.
#[allow(clippy::too_many_arguments)]
fn pointer(
    ui: &Ui,
    response: &Response,
    map: &Map,
    spline: &dyn Spline,
    defaults: &Points,
    max_points: usize,
    mem: &mut Memory,
    ed: &mut Edit<'_, '_>,
) {
    let hit_point = |pos: Pos2, points: &[[f32; 2]]| {
        points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, map.to_screen(*p).distance(pos)))
            .filter(|(_, d)| *d <= HIT)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let pressed = ui.input(|i| i.pointer.primary_pressed());
    let pos = response.interact_pointer_pos().or(response.hover_pos());
    if pressed
        && response.is_pointer_button_down_on()
        && let Some(pos) = pos
    {
        if let Some(i) = hit_point(pos, ed.points) {
            mem.selected = Some(i);
            mem.grab = Some(Grab {
                start: ed.points.clone(),
                index: i,
                origin: ed.points[i],
                moved: [0.0, 0.0],
                cause: Cause::Drag,
            });
        } else {
            let at = map.unit_at(pos);
            let on_curve = map
                .to_screen([at[0], spline.eval(ed.points, at[0])])
                .distance(pos)
                <= HIT;
            if on_curve && ed.points.len() < max_points && (0.0..=1.0).contains(&at[0]) {
                let start = ed.points.clone();
                let mut to = start.clone();
                let y = spline.eval(&start, at[0]);
                if let Some(i) = insert(&mut to, [at[0], y]) {
                    let p = to[i];
                    if ed.propose(to, Cause::Add, false) {
                        mem.selected = Some(i);
                        mem.grab = Some(Grab {
                            start,
                            index: i,
                            origin: p,
                            moved: [0.0, 0.0],
                            cause: Cause::Add,
                        });
                    }
                }
            }
        }
    }
    match change::gesture(ui, response) {
        Gesture::None => {}
        Gesture::Moved(d) => {
            if let Some(mut grab) = mem.grab.take() {
                ed.drag(&mut grab, map.delta(d));
                mem.grab = Some(grab);
            }
        }
        Gesture::Ended => {
            if let Some(grab) = mem.grab.take() {
                ed.finish(grab);
            }
        }
        Gesture::Cancelled => {
            if let Some(grab) = mem.grab.take() {
                ed.cancel(grab);
            }
        }
    }
    // A press and release without a drag: the gesture (a selection, a
    // point added) ends.
    if (response.clicked() || response.secondary_clicked())
        && let Some(grab) = mem.grab.take()
    {
        ed.finish(grab);
    }
    let delete_at = |pos: Option<Pos2>, points: &[[f32; 2]]| pos.and_then(|p| hit_point(p, points));
    if response.double_clicked() || response.secondary_clicked() {
        match delete_at(pos, ed.points) {
            Some(i) if ed.points.len() > 2 => {
                let mut to = ed.points.clone();
                to.remove(i);
                if ed.propose(to, Cause::Delete, true) {
                    mem.selected = Some(i.min(ed.points.len() - 1));
                }
            }
            None if response.double_clicked() => {
                ed.propose(defaults.clone(), Cause::Reset, true);
                mem.selected = None;
            }
            _ => {}
        }
    }
}

/// The value picked in the image.
fn picked(
    p: Pick,
    spline: &dyn Spline,
    max_points: usize,
    mem: &mut Memory,
    ed: &mut Edit<'_, '_>,
) {
    match p.action {
        PickAction::Hover => {}
        PickAction::Press => {
            let start = ed.points.clone();
            let index = match nearest(ed.points, p.x, PICK_TOLERANCE) {
                Some(i) => Some(i),
                None if ed.points.len() < max_points => {
                    let mut to = start.clone();
                    let y = spline.eval(&start, p.x);
                    insert(&mut to, [p.x, y]).filter(|_| ed.propose(to.clone(), Cause::Pick, false))
                }
                None => None,
            };
            if let Some(i) = index {
                mem.selected = Some(i);
                mem.grab = Some(Grab {
                    start,
                    index: i,
                    origin: ed.points[i],
                    moved: [0.0, 0.0],
                    cause: Cause::Pick,
                });
            }
        }
        PickAction::Move(dy) => {
            if let Some(mut grab) = mem.grab.take() {
                ed.drag(&mut grab, [0.0, dy]);
                mem.grab = Some(grab);
            }
        }
        PickAction::Release => {
            if let Some(grab) = mem.grab.take() {
                ed.finish(grab);
            }
        }
        PickAction::Cancel => {
            if let Some(grab) = mem.grab.take() {
                ed.cancel(grab);
            }
        }
    }
}

/// How close a picked value must be to a point's input to take it.
pub const PICK_TOLERANCE: f32 = 0.04;

/// The keys, when focused.
fn keyboard(
    ui: &mut Ui,
    spline: &dyn Spline,
    defaults: &Points,
    step: f32,
    max_points: usize,
    mem: &mut Memory,
    ed: &mut Edit<'_, '_>,
) {
    let k = keys(ui.ctx());
    let take = |key: Key| ui.input_mut(|i| i.consume_key(Modifiers::NONE, key));
    let n = ed.points.len();
    if take(k.next) {
        mem.selected = Some(mem.selected.map_or(0, |i| (i + 1).min(n - 1)));
    }
    if take(k.previous) {
        mem.selected = Some(mem.selected.map_or(n - 1, |i| i.saturating_sub(1)));
    }
    if take(k.reset) {
        ed.propose(defaults.clone(), Cause::Reset, true);
        mem.selected = None;
    }
    let n = ed.points.len();
    if take(k.add) && n < max_points {
        let i = mem.selected.unwrap_or(0).min(n - 2);
        let x = (ed.points[i][0] + ed.points[i + 1][0]) / 2.0;
        let mut to = ed.points.clone();
        if let Some(j) = insert(&mut to, [x, spline.eval(ed.points, x)])
            && ed.propose(to, Cause::Add, true)
        {
            mem.selected = Some(j);
        }
    }
    if (take(k.delete[0]) || take(k.delete[1]))
        && let Some(i) = mem.selected
        && ed.points.len() > 2
    {
        let mut to = ed.points.clone();
        to.remove(i);
        if ed.propose(to, Cause::Delete, true) {
            mem.selected = Some(i.min(ed.points.len() - 1));
        }
    }
    let [dx, dy] = change::arrows(ui);
    if (dx != 0.0 || dy != 0.0)
        && let Some(i) = mem.selected
        && i < ed.points.len()
    {
        let s = step * change::scale(ui);
        let p = ed.points[i];
        let mut to = ed.points.clone();
        to[i] = moved(&to, i, [p[0] + dx * s, p[1] + dy * s]);
        ed.propose(to, Cause::Key, true);
    }
}

/// Assistive technologies: increment, decrement or set the selected
/// point's output.
fn access(ui: &Ui, id: Id, step: f32, mem: &mut Memory, ed: &mut Edit<'_, '_>) {
    let Some(i) = mem.selected.filter(|i| *i < ed.points.len()) else {
        return;
    };
    let steps = change::access_steps(ui, id);
    let set = ui.input(|inp| {
        use egui::accesskit::{Action, ActionData};
        inp.accesskit_action_requests(id, Action::SetValue)
            .filter_map(|r| match r.data {
                Some(ActionData::NumericValue(n)) => Some(n as f32),
                _ => None,
            })
            .last()
    });
    let p = ed.points[i];
    let y = set.unwrap_or(p[1] + steps * step);
    if y != p[1] {
        let mut to = ed.points.clone();
        to[i] = moved(&to, i, [p[0], y]);
        ed.propose(to, Cause::Access, true);
    }
}

/// The background, the grid, the histogram, the identity, the curve,
/// the points and the pick.
#[allow(clippy::too_many_arguments)]
fn paint(
    ui: &Ui,
    rect: Rect,
    area: Rect,
    response: &Response,
    points: &[[f32; 2]],
    spline: &dyn Spline,
    histogram: Option<&[f32]>,
    pick: Option<Pick>,
    color: Option<Color32>,
    mem: &Memory,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let t = CurveTokens::of(ui.ctx());
    let tokens = Tokens::of(ui.ctx());
    let painter = ui.painter_at(rect);
    let map = Map { area };
    let radius = tokens.radius.widget;
    painter.rect_filled(rect, radius, t.background);
    let hair = 1.0 / ui.ctx().pixels_per_point();
    for q in 1..4 {
        let f = q as f32 / 4.0;
        let x = area.left() + area.width() * f;
        let y = area.bottom() - area.height() * f;
        painter.line_segment(
            [pos2(x, area.top()), pos2(x, area.bottom())],
            Stroke::new(hair, t.grid),
        );
        painter.line_segment(
            [pos2(area.left(), y), pos2(area.right(), y)],
            Stroke::new(hair, t.grid),
        );
    }
    if let Some(bins) = histogram.filter(|b| !b.is_empty()) {
        let max = bins.iter().copied().fold(0.0f32, f32::max);
        let h = crate::histogram::heights(bins, max, true);
        let n = h.len() as f32;
        let fill = t.grid.gamma_multiply(0.6);
        for (i, v) in h.iter().enumerate() {
            let x0 = area.left() + area.width() * i as f32 / n;
            let x1 = area.left() + area.width() * (i + 1) as f32 / n;
            let top = area.bottom() - area.height() * v;
            painter.rect_filled(
                Rect::from_min_max(pos2(x0, top), pos2(x1, area.bottom())),
                0.0,
                fill,
            );
        }
    }
    painter.line_segment(
        [area.left_bottom(), area.right_top()],
        Stroke::new(1.0, t.identity),
    );
    let line_color = color.unwrap_or(t.line);
    if points.len() >= 2 {
        let samples = (area.width().round() as usize).clamp(32, 1024);
        let line: Vec<Pos2> = (0..=samples)
            .map(|s| {
                let x = s as f32 / samples as f32;
                map.to_screen([x, spline.eval(points, x).clamp(0.0, 1.0)])
            })
            .collect();
        painter.add(Shape::line(line, Stroke::new(1.5, line_color)));
    }
    let picked = pick.and_then(|p| {
        let x = p.x.clamp(0.0, 1.0);
        let at = map.to_screen([x, 0.0]).x;
        painter.line_segment(
            [pos2(at, area.top()), pos2(at, area.bottom())],
            Stroke::new(1.0, t.pick),
        );
        if points.len() >= 2 {
            let y = spline.eval(points, x).clamp(0.0, 1.0);
            painter.circle_stroke(map.to_screen([x, y]), 3.0, Stroke::new(1.0, t.pick));
        }
        nearest(points, x, PICK_TOLERANCE)
    });
    let hovered = response
        .hover_pos()
        .and_then(|pos| {
            points
                .iter()
                .enumerate()
                .map(|(i, p)| (i, map.to_screen(*p).distance(pos)))
                .filter(|(_, d)| *d <= HIT)
                .min_by(|a, b| a.1.total_cmp(&b.1))
        })
        .map(|(i, _)| i);
    for (i, p) in points.iter().enumerate() {
        let c = map.to_screen(*p);
        let selected = mem.selected == Some(i);
        let lit = selected || picked == Some(i) || hovered == Some(i);
        let r = if lit { 5.0 } else { 4.0 };
        if selected {
            painter.circle_filled(c, r, t.point_selected);
        } else {
            painter.circle_filled(c, r, t.background);
            let ring = if lit { t.point_selected } else { t.point };
            painter.circle_stroke(c, r, Stroke::new(1.5, ring));
        }
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(2.0, tokens.focus_ring),
            StrokeKind::Inside,
        );
    }
}

/// What the editor shows, for screen readers.
fn value_text(ctx: &egui::Context, points: &[[f32; 2]], selected: Option<usize>) -> String {
    let n = points.len().to_string();
    match selected.and_then(|i| points.get(i).map(|p| (i, p))) {
        Some((i, p)) => {
            let pc = |v: f32| strings::number(ctx, f64::from(v) * 100.0, 0);
            strings::tr_args(
                ctx,
                &crate::strings::CURVE_VALUE_SELECTED,
                &[
                    ("point", &(i + 1).to_string()),
                    ("points", &n),
                    ("input", &pc(p[0])),
                    ("output", &pc(p[1])),
                ],
            )
        }
        None => strings::tr_args(ctx, &crate::strings::CURVE_VALUE, &[("points", &n)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splines_go_through_their_points_and_stay_flat_past_the_ends() {
        let pts = vec![[0.1f32, 0.2], [0.5, 0.7], [0.9, 0.8]];
        for s in [&Linear as &dyn Spline, &MonotoneCubic] {
            for p in &pts {
                assert!((s.eval(&pts, p[0]) - p[1]).abs() < 1e-5);
            }
            assert_eq!(s.eval(&pts, 0.0), 0.2);
            assert_eq!(s.eval(&pts, 1.0), 0.8);
        }
        assert!((Linear.eval(&pts, 0.3) - 0.45).abs() < 1e-5);
    }

    #[test]
    fn the_monotone_cubic_never_overshoots() {
        let pts = vec![[0.0f32, 0.0], [0.4, 0.1], [0.5, 0.9], [1.0, 1.0]];
        let mut last = -1.0;
        for i in 0..=200 {
            let y = MonotoneCubic.eval(&pts, i as f32 / 200.0);
            assert!(y >= last - 1e-6, "falls at {i}: {y} < {last}");
            assert!((-1e-6..=1.0 + 1e-6).contains(&y));
            last = y;
        }
    }

    #[test]
    fn points_stay_ordered_and_apart() {
        let mut pts = identity();
        assert_eq!(insert(&mut pts, [0.5, 0.6]), Some(1));
        assert_eq!(insert(&mut pts, [0.505, 0.6]), None, "too close");
        assert_eq!(moved(&pts, 1, [2.0, -1.0]), [1.0 - MIN_GAP, 0.0]);
        assert_eq!(moved(&pts, 0, [0.7, 0.5]), [0.5 - MIN_GAP, 0.5]);
        assert_eq!(nearest(&pts, 0.52, 0.04), Some(1));
        assert_eq!(nearest(&pts, 0.3, 0.04), None);
    }
}
