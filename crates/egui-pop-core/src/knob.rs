//! A knob: one value on a rotary dial, from the same terms as a
//! [`Slider`](crate::slider::Slider) (soft and hard range, step, unit,
//! default, the `on_change` hook) and under the same history contract: a
//! drag is one gesture, its frames `done: false` and its end once more
//! `done: true` from the value before it; Escape during it puts the value
//! back with no trace.
//!
//! Gestures:
//!
//! - **Drag up or down** from the dial's centre: up is more, the soft
//!   range over [`Knob::travel`] points (200 by default). A click never
//!   makes the value jump.
//! - **Drag around** from the dial's rim: the value follows the
//!   pointer's turn, relative to where the press was (the soft range over
//!   the dial's 270° sweep).
//! - **Fine**: Shift, or the pointer profile's fine modifier, makes a
//!   drag ten times finer, from where it is (no jump).
//! - **Wheel**, **keys** (arrows, Page Up and Page Down, Home and End),
//!   **typed entry** (Enter) and **assistive actions**: as the slider's,
//!   from the same pointer profile.
//! - **Double click**: back to the default.
//!
//! **Layout.** The dial takes the largest square of its room. In a room
//! at least twice as wide as high, the label and the value stand beside
//! it; in one taller than wide, the value is above and the label below
//! (as a fader's); else the value is inside the dial and the label in
//! the tooltip.
//!
//! **Accessibility.** An AccessKit slider named by its label, with its
//! value, value text, range, step and the increment, decrement and
//! set-value actions, as the slider's.
//!
//! ```
//! use egui_pop_core::knob::Knob;
//!
//! fn knob(ui: &mut egui::Ui, ev: &mut f64) {
//!     Knob::new(ev, "Exposure", -3.0..=4.0).step(0.01).unit("EV").signed().default(0.0).show(ui);
//! }
//! ```

use std::f32::consts::PI;
use std::ops::RangeInclusive;
use std::sync::Arc;

use egui::{
    Color32, Galley, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, Widget, WidgetInfo,
    pos2, vec2,
};

use crate::kit::{Kit, State, Tone};
use crate::slider::{
    Cause, Change, Drag, Hook, Spec, Thumb, Verdict, access, decide, gestures, keep_auto_ids,
    keys_wheel_access, modifier_scale, open_typed_entry, spec_builders, state_of, typed_entry,
    typing_id, typing_text,
};
use crate::state::Slot;

/// The dial's sweep, in radians: from 7:30 to 4:30 through 12.
pub const SWEEP: f32 = 1.5 * PI;

/// Points of vertical drag over the soft range, by default.
pub const TRAVEL: f32 = 200.0;

/// A drag's state, from its press to its end.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Turn {
    /// The value before the drag.
    start: f64,
    /// Where the drag has led, as a place on the soft range (0 to 1),
    /// accumulated exactly.
    frac: f64,
    /// Pressed on the rim: the drag goes around.
    around: bool,
    /// The pointer's last angle, for a drag around.
    angle: f32,
}

const TURN: Slot<Turn> = Slot::new("core.knob.turn");

/// A rotary knob over an `f64`.
#[derive(Debug)]
pub struct Knob<'a> {
    value: &'a mut f64,
    spec: Spec,
    default: Option<f64>,
    hook: Option<Hook<'a>>,
    size: Option<Vec2>,
    travel: f32,
}

impl<'a> Knob<'a> {
    /// A knob spanning `soft`, which is also its hard range until
    /// [`Knob::hard`] widens it.
    pub fn new(value: &'a mut f64, label: &str, soft: RangeInclusive<f64>) -> Self {
        Self {
            value,
            spec: Spec::new(label, soft),
            default: None,
            hook: None,
            size: None,
            travel: TRAVEL,
        }
    }

    spec_builders!();

    /// What a double click goes back to.
    pub fn default(mut self, default: f64) -> Self {
        self.default = Some(default);
        self
    }

    /// Sees every change before it is written, as the slider's hook.
    pub fn on_change(mut self, hook: impl FnMut(&Change) -> Verdict + 'a) -> Self {
        self.hook = Some(Hook(Box::new(hook)));
        self
    }

    /// Its room, in points (a slot). By default two rows high and as wide
    /// as the dial with its label and value beside it.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// Points of vertical drag over the soft range.
    pub fn travel(mut self, points: f32) -> Self {
        if points > 0.0 {
            self.travel = points;
        }
        self
    }

    /// Draws the knob; says what changed.
    pub fn show(self, ui: &mut Ui) -> crate::slider::SliderOutput {
        if self.spec.state.is_disabled() {
            return ui
                .scope(|ui| {
                    ui.disable();
                    self.draw(ui)
                })
                .inner;
        }
        self.draw(ui)
    }

    fn draw(self, ui: &mut Ui) -> crate::slider::SliderOutput {
        let Knob {
            value,
            spec,
            default,
            mut hook,
            size,
            travel,
        } = self;
        let row = ui.spacing().interact_size.y;
        let wanted = size.unwrap_or(vec2(row * 2.0 * 3.0, row * 2.0));
        let (id, rect) = ui.allocate_space(vec2(wanted.x.max(row), wanted.y.max(row)));
        let layout = Layout::of(rect);
        let mut response = ui.interact(rect, id, Sense::click_and_drag());
        let before = *value;
        let mut changes = Vec::new();
        let mut apply = |change: Change, value: &mut f64| -> bool {
            match decide(&mut hook, change, &spec) {
                Some(c) => {
                    *value = c.to;
                    changes.push(c);
                    true
                }
                None => false,
            }
        };
        let change = |from: f64, to: f64, cause: Cause, done: bool| Change {
            thumb: Thumb::Value,
            from,
            to,
            cause,
            done,
        };
        let typing = typing_id(id);
        if let Some(text) = typing_text(ui.ctx(), typing) {
            let entry = Rect::from_center_size(rect.center(), vec2(rect.width(), row));
            if let Some(end) = typed_entry(ui, entry, typing, text, &spec, *value) {
                if let Some(v) = end.value {
                    let to = spec.settle(v, spec.hard);
                    if to != *value {
                        apply(change(*value, to, Cause::Typed, true), value);
                    }
                }
                if end.refocus {
                    response.request_focus();
                }
            }
        } else {
            keep_auto_ids(ui);
            let g = gestures(ui.ctx());
            let shift = ui.input(|i| i.modifiers.shift);
            let fine = if shift {
                0.1
            } else {
                modifier_scale(ui, &g).min(1.0)
            };
            match turn(ui, &response, &layout, &spec, *value, fine, travel) {
                Drag::None => {}
                Drag::Moved(v) => {
                    let to = spec.settle(v, spec.hard);
                    if to != *value {
                        apply(change(*value, to, Cause::Drag, false), value);
                    }
                }
                Drag::Ended(start) => {
                    if start != *value && !apply(change(start, *value, Cause::Drag, true), value) {
                        *value = start;
                        crate::binding::cancel_gesture(ui.ctx(), response.id);
                    }
                }
                Drag::Cancelled(start) => {
                    *value = start;
                    crate::binding::cancel_gesture(ui.ctx(), response.id);
                }
            }
            if g.reset_on_double_click
                && response.double_clicked()
                && let Some(d) = default
                && d != *value
            {
                apply(change(*value, d, Cause::Reset, true), value);
            }
            if response.clicked() {
                response.request_focus();
            }
            let scale = modifier_scale(ui, &g);
            if let Some((v, cause)) =
                keys_wheel_access(ui, &response, &spec, *value, spec.soft, scale, &g)
            {
                let to = spec.settle(v, spec.hard);
                if to != *value {
                    apply(change(*value, to, cause, true), value);
                }
            }
            if response.has_focus()
                && spec.state.is_editable()
                && ui.input(|i| i.key_pressed(g.typed_entry))
            {
                open_typed_entry(ui, typing, spec.number(ui.ctx(), *value));
            }
            paint(ui, &layout, &response, &spec, *value);
        }
        if *value != before {
            response.mark_changed();
        }
        let enabled = ui.is_enabled();
        let v = *value;
        response.widget_info(|| WidgetInfo::slider(enabled, v, &spec.label));
        access(ui, response.id, &spec, v, spec.soft);
        state_of(ui, &[&response], &spec, "knob");
        let mut response = crate::field::hover(ui, &response, &spec.state);
        if layout.label.is_none() {
            response = response.on_hover_text(&spec.label);
        }
        crate::slider::SliderOutput {
            response,
            changes,
            quad: None,
        }
    }
}

impl Widget for Knob<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

/// Where a knob's parts go in its room.
#[derive(Clone, Copy, Debug)]
struct Layout {
    dial: Rect,
    /// Where the value goes; `None`: inside the dial.
    value: Option<Rect>,
    /// Where the label goes; `None`: in the tooltip.
    label: Option<Rect>,
}

impl Layout {
    fn of(rect: Rect) -> Self {
        let (w, h) = (rect.width(), rect.height());
        if w >= 2.0 * h {
            // Beside: the dial at the start, the label over the value.
            let dial = Rect::from_min_size(rect.min, vec2(h, h));
            let text = Rect::from_min_max(pos2(dial.right() + 4.0, rect.top()), rect.max);
            let (label, value) = text.split_top_bottom_at_fraction(0.5);
            Self {
                dial,
                value: Some(value),
                label: Some(label),
            }
        } else if h >= 1.6 * w {
            // A fader's order: the value above, the label below.
            let band = (h - w) / 2.0;
            let value = Rect::from_min_size(rect.min, vec2(w, band));
            let dial = Rect::from_min_size(pos2(rect.left(), value.bottom()), vec2(w, w));
            let label = Rect::from_min_max(pos2(rect.left(), dial.bottom()), rect.max);
            Self {
                dial,
                value: Some(value),
                label: Some(label),
            }
        } else {
            let side = w.min(h);
            Self {
                dial: Rect::from_center_size(rect.center(), vec2(side, side)),
                value: None,
                label: None,
            }
        }
    }

    fn radius(&self) -> f32 {
        (self.dial.width().min(self.dial.height()) / 2.0 - 2.0).max(1.0)
    }
}

/// The angle of a place on the soft range (0 to 1), in screen radians
/// (y down): 0 at 7:30, 1 at 4:30, through 12.
fn angle_of(frac: f32) -> f32 {
    // 7:30 is 135° below the right, clockwise on screen.
    PI * 0.75 + frac.clamp(0.0, 1.0) * SWEEP
}

/// The pointer's angle around `centre`, in screen radians.
fn pointer_angle(centre: Pos2, at: Pos2) -> f32 {
    (at.y - centre.y).atan2(at.x - centre.x)
}

/// A drag's effect in a frame: up and down from the centre, around from
/// the rim; `fine` scales the motion.
fn turn(
    ui: &Ui,
    response: &Response,
    layout: &Layout,
    spec: &Spec,
    value: f64,
    fine: f64,
    travel: f32,
) -> Drag {
    let slot = TURN.keyed(response.id);
    let centre = layout.dial.center();
    crate::pointer::drag_cursor(response, crate::pointer::DragKind::Value);
    if response.drag_started() {
        let at = response.interact_pointer_pos().unwrap_or(centre);
        let around = at.distance(centre) > layout.radius() * 0.6;
        slot.set(
            ui.ctx(),
            Turn {
                start: value,
                frac: spec.frac(value),
                around,
                angle: pointer_angle(centre, at),
            },
        );
    }
    if response.drag_stopped() {
        let start = slot.get(ui.ctx()).map_or(value, |t| t.start);
        slot.remove(ui.ctx());
        let cancel = gestures(ui.ctx()).cancel;
        return if ui.input(|i| i.key_pressed(cancel)) {
            Drag::Cancelled(start)
        } else {
            Drag::Ended(start)
        };
    }
    if response.dragged()
        && let Some(mut t) = slot.get(ui.ctx())
    {
        let delta = if t.around {
            let Some(at) = response.interact_pointer_pos() else {
                return Drag::None;
            };
            let now = pointer_angle(centre, at);
            let mut d = now - t.angle;
            // The short way round.
            if d > PI {
                d -= 2.0 * PI;
            } else if d < -PI {
                d += 2.0 * PI;
            }
            t.angle = now;
            f64::from(d / SWEEP)
        } else {
            f64::from(-response.drag_delta().y / travel.max(1.0))
        } * fine;
        if delta != 0.0 {
            let (lo, hi) = (spec.frac(spec.soft.0), spec.frac(spec.soft.1));
            t.frac = (t.frac + delta).clamp(lo.min(t.frac), hi.max(t.frac));
            slot.set(ui.ctx(), t);
            return Drag::Moved(spec.value_at(t.frac));
        }
        slot.set(ui.ctx(), t);
    }
    Drag::None
}

/// The dial: the track's arc, the value's arc (from zero when the soft
/// range holds it, else from the start), the pointer, the focus ring;
/// then the label and the value where the layout puts them. Drawn with
/// the slider's kit parts: `slider` (the dial), `slider-fill` (the
/// value's arc), `slider-thumb` (the pointer), `slider-value` (the
/// text).
fn paint(ui: &Ui, layout: &Layout, response: &Response, spec: &Spec, v: f64) {
    let room = layout
        .dial
        .union(layout.value.unwrap_or(layout.dial))
        .union(layout.label.unwrap_or(layout.dial));
    if !ui.is_rect_visible(room) {
        return;
    }
    let kit = Kit::of(ui.ctx());
    let past = v < spec.soft.0 || v > spec.soft.1;
    let tone = if past { Tone::Warning } else { Tone::Neutral };
    let state = State::from_response(response).toned(tone);
    let dial = kit.slider.style(state);
    let fill = kit.slider_fill.style(state);
    let thumb = kit.slider_thumb.style(state);
    let painter = ui.painter_at(room.expand(4.0));
    let centre = layout.dial.center();
    let r = layout.radius();
    let width = (r * 0.18).clamp(2.0, 5.0);
    let arc = |from: f32, to: f32, radius: f32| -> Vec<Pos2> {
        let n = (((to - from).abs() / SWEEP) * 48.0).ceil().max(2.0) as usize;
        (0..=n)
            .map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                centre + radius * vec2(a.cos(), a.sin())
            })
            .collect()
    };
    // The body, then the track's arc.
    painter.circle(
        centre,
        r - width,
        dial.fill,
        Stroke::new(dial.border_width, dial.border_color),
    );
    let track = if dial.border_color == Color32::TRANSPARENT {
        dial.text.gamma_multiply(0.3)
    } else {
        dial.border_color
    };
    painter.add(Shape::line(
        arc(angle_of(0.0), angle_of(1.0), r - width / 2.0),
        Stroke::new(width, track),
    ));
    let at = spec.frac(v) as f32;
    let zero = if spec.soft.0 < 0.0 && spec.soft.1 > 0.0 {
        spec.frac(0.0) as f32
    } else {
        0.0
    };
    let fill_color = if fill.fill == Color32::TRANSPARENT {
        dial.text
    } else {
        fill.fill
    };
    if (at - zero).abs() > f32::EPSILON {
        painter.add(Shape::line(
            arc(angle_of(zero), angle_of(at), r - width / 2.0),
            Stroke::new(width, fill_color),
        ));
    }
    let a = angle_of(at);
    let pointer = if thumb.fill == Color32::TRANSPARENT {
        dial.text
    } else {
        thumb.fill
    };
    painter.line_segment(
        [
            centre + (r - width) * 0.25 * vec2(a.cos(), a.sin()),
            centre + (r - width) * 0.9 * vec2(a.cos(), a.sin()),
        ],
        Stroke::new(width.max(2.0), pointer),
    );
    if response.has_focus() {
        let ring = kit.slider.style(state.with(State::FOCUS, true));
        let color = if ring.ring_width > 0.0 {
            ring.ring_color
        } else {
            ui.visuals().selection.stroke.color
        };
        painter.circle_stroke(
            centre,
            r + 1.5,
            Stroke::new(ring.ring_width.max(1.5), color),
        );
    }
    let font = egui::TextStyle::Button.resolve(ui.style());
    let fit = |text: String, room: Rect, color: Color32| -> Arc<Galley> {
        let mut job = egui::text::LayoutJob::simple_singleline(text, font.clone(), color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(room.width().max(1.0));
        ui.painter().layout_job(job)
    };
    let place = |g: &Arc<Galley>, room: Rect, centred: bool| {
        let x = if centred {
            room.center().x - g.size().x / 2.0
        } else {
            room.left()
        };
        painter.galley(
            pos2(x, room.center().y - g.size().y / 2.0),
            g.clone(),
            dial.text,
        );
    };
    let text = spec.text(ui.ctx(), v);
    let centred = layout
        .value
        .is_some_and(|r| r.width() < 2.0 * layout.dial.width());
    if let Some(room) = layout.value {
        let mut g = fit(text, room, dial.text);
        if g.elided {
            g = fit(spec.number(ui.ctx(), v), room, dial.text);
        }
        place(&g, room, centred);
    } else if r > font.size * 1.2 {
        // Inside the dial: the number only.
        let inner = Rect::from_center_size(centre, Vec2::splat(r * 1.3));
        let g = fit(spec.number(ui.ctx(), v), inner, dial.text);
        place(&g, inner, true);
    }
    if let Some(room) = layout.label {
        let g = fit(spec.label.clone(), room, dial.text.gamma_multiply(0.75));
        place(&g, room, centred);
    }
}
