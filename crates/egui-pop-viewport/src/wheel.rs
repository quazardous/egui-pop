//! Colour wheels: the lift, gamma and gain wheels of colour grading,
//! and a hue and saturation picker, each with a luminance slider
//! (egui-pop's slider, `egui_pop_core::slider`).
//!
//! A wheel's value is a [`WheelValue`]: a hue (degrees), a chroma (0 at
//! the centre, 1 at the rim) and a luminance. What they mean is the
//! application's (an offset added to shadows, a power, a gain, or a
//! colour); the wheel draws them and handles the gestures.
//!
//! - [`ColorWheel::offset`]: neutral at the centre; a drag moves the
//!   puck relatively (a click never makes it jump; Ctrl finer, Shift
//!   coarser), Escape puts it back, a double click resets it; the
//!   luminance slider is signed, around zero.
//! - [`ColorWheel::picker`]: a press picks the hue and saturation under
//!   the pointer, a drag follows it; the luminance slider runs from 0 to
//!   1.
//! - **Keys** when focused: the arrows move the puck by a step.
//!
//! Every change, of the puck or of the luminance, goes through the
//! [`on_change`](ColorWheel::on_change) hook as a [`Change`] of the whole
//! value: a drag's frames with `done: false`, its end once more from the
//! value before it (a veto puts it back), and each key, reset and typed
//! luminance. `egui-pop-imaging` runs the finished ones as the
//! `wheel.change` step of an application.
//!
//! **Accessibility.** The disc is an AccessKit slider named by its
//! label, whose value says the hue, the chroma and the luminance;
//! increment and decrement change the chroma. The luminance slider is
//! named after the wheel ("Lift luminance").
//!
//! ```
//! use egui_pop_viewport::wheel::{ColorWheel, WheelValue};
//!
//! fn grading(ui: &mut egui::Ui, wheels: &mut [WheelValue; 3]) {
//!     ui.horizontal(|ui| {
//!         for (value, label) in wheels.iter_mut().zip(["Lift", "Gamma", "Gain"]) {
//!             ColorWheel::offset(value, label).size(120.0).show(ui);
//!         }
//!     });
//! }
//! ```

use std::ops::RangeInclusive;

use egui::ecolor::Hsva;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Response, Role, Sense, Shape, Stroke, Ui, Vec2, WidgetInfo, vec2};
use egui_pop_core::slider::{self, Slider};
use egui_pop_core::state::Slot;
use egui_pop_core::strings;

/// A drag under way on a wheel, per wheel.
const GRAB: Slot<Grab> = Slot::new("viewport.wheel.grab");
use egui_pop_core::theme::Tokens;
use serde_json::{Value as Json, json};

use crate::change::{self, Cause, Change, Gesture, Hook, StepValue, Verdict};

/// A wheel's value: a hue in degrees (0 to the right, counter-clockwise),
/// a chroma (0 at the centre, 1 at the rim) and a luminance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelValue {
    pub hue: f32,
    pub chroma: f32,
    pub luminance: f32,
}

impl WheelValue {
    /// The puck's position on the unit disc, `[x, y]` (y up).
    pub fn xy(&self) -> [f32; 2] {
        let a = self.hue.to_radians();
        [self.chroma * a.cos(), self.chroma * a.sin()]
    }

    /// The value with the puck at `[x, y]` (y up), kept in the disc; the
    /// hue is kept at the centre.
    pub fn with_xy(self, [x, y]: [f32; 2]) -> WheelValue {
        let chroma = (x * x + y * y).sqrt();
        let hue = if chroma < 1e-6 {
            self.hue
        } else {
            y.atan2(x).to_degrees().rem_euclid(360.0)
        };
        WheelValue {
            hue,
            chroma: chroma.min(1.0),
            ..self
        }
    }
}

/// `{"hue": <degrees>, "chroma": <0..1>, "luminance": <value>}`.
impl StepValue for WheelValue {
    fn to_json(&self) -> Json {
        json!({ "hue": self.hue, "chroma": self.chroma, "luminance": self.luminance })
    }

    fn from_json(v: &Json) -> Option<Self> {
        Some(WheelValue {
            hue: change::json_f32(v.get("hue")?)?,
            chroma: change::json_f32(v.get("chroma")?)?,
            luminance: change::json_f32(v.get("luminance")?)?,
        })
    }
}

/// How the disc answers the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelMode {
    /// Neutral at the centre; drags are relative (lift, gamma, gain).
    Offset,
    /// A press picks the colour under the pointer (hue and saturation).
    Picker,
}

/// The wheels' colours, from the theme (`wheel.*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelTokens {
    pub ring: Color32,
    pub puck: Color32,
    pub puck_outline: Color32,
}

impl WheelTokens {
    pub fn of(ctx: &egui::Context) -> WheelTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        WheelTokens {
            ring: c("wheel.ring", t.border.strong),
            puck: c("wheel.puck", t.text.strong),
            puck_outline: c("wheel.puck-outline", t.surface.canvas),
        }
    }
}

/// A colour wheel, with its luminance slider.
#[derive(Debug)]
pub struct ColorWheel<'a> {
    value: &'a mut WheelValue,
    label: String,
    mode: WheelMode,
    default: WheelValue,
    size: f32,
    luminance: Option<RangeInclusive<f64>>,
    step: f32,
    hook: Option<Hook<'a, WheelValue>>,
}

/// What a wheel did in a frame.
#[derive(Debug)]
pub struct WheelOutput {
    /// The disc; `changed()` when the value changed.
    pub response: Response,
    /// The luminance slider, when shown.
    pub luminance: Option<Response>,
    /// The changes written this frame, after the hook.
    pub changes: Vec<Change<WheelValue>>,
}

/// A gesture under way: the value before it, and where the pointer took
/// the puck.
#[derive(Clone, Copy, Debug)]
struct Grab {
    start: WheelValue,
    at: [f32; 2],
    cause: Cause,
}

impl<'a> ColorWheel<'a> {
    fn new(value: &'a mut WheelValue, label: &str, mode: WheelMode) -> ColorWheel<'a> {
        let luminance = match mode {
            WheelMode::Offset => -1.0..=1.0,
            WheelMode::Picker => 0.0..=1.0,
        };
        ColorWheel {
            value,
            label: label.to_string(),
            mode,
            default: WheelValue::default(),
            size: 160.0,
            luminance: Some(luminance),
            step: 0.01,
            hook: None,
        }
    }

    /// A grading wheel (lift, gamma, gain): neutral at the centre,
    /// relative drags, a signed luminance from -1 to 1.
    pub fn offset(value: &'a mut WheelValue, label: &str) -> ColorWheel<'a> {
        ColorWheel::new(value, label, WheelMode::Offset)
    }

    /// A hue and saturation picker: a press picks, a luminance from 0 to
    /// 1.
    pub fn picker(value: &'a mut WheelValue, label: &str) -> ColorWheel<'a> {
        let mut w = ColorWheel::new(value, label, WheelMode::Picker);
        w.default.luminance = 0.5;
        w
    }

    /// What a double click goes back to (zero; a picker's luminance at
    /// a half).
    pub fn default(mut self, default: WheelValue) -> Self {
        self.default = default;
        self
    }

    /// The disc's diameter in points (160 by default); the slider is as
    /// wide.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size.max(48.0);
        self
    }

    /// The luminance slider's range, or no slider (`None`).
    pub fn luminance(mut self, range: Option<RangeInclusive<f64>>) -> Self {
        self.luminance = range;
        self
    }

    /// How far an arrow key moves the puck, in radii (a hundredth by
    /// default).
    pub fn step(mut self, step: f32) -> Self {
        self.step = step.abs();
        self
    }

    /// Sees each change before it is written: allow, veto or redirect
    /// (another value; for a change of the luminance, its luminance is
    /// taken).
    pub fn on_change(
        mut self,
        hook: impl FnMut(&Change<WheelValue>) -> Verdict<WheelValue> + 'a,
    ) -> Self {
        self.hook = Some(Hook::new(hook));
        self
    }

    /// Draws the wheel and its slider; says what changed.
    pub fn show(self, ui: &mut Ui) -> WheelOutput {
        let ColorWheel {
            value,
            label,
            mode,
            default,
            size,
            luminance,
            step,
            mut hook,
        } = self;
        let mut changes = Vec::new();
        let inner = ui.vertical(|ui| {
            ui.set_width(size);
            let (rect, mut response) =
                ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
            let radius = size / 2.0 - 4.0;
            let center = rect.center();
            let before = *value;
            disc(
                ui,
                &response,
                center,
                radius,
                mode,
                default,
                step,
                value,
                &mut hook,
                &mut changes,
            );
            if response.clicked() || response.drag_started() {
                response.request_focus();
            }
            if *value != before {
                response.mark_changed();
            }
            paint(ui, &response, center, radius, mode, *value);
            let text = value_text(ui.ctx(), *value, mode);
            let enabled = ui.is_enabled();
            response.widget_info(|| {
                let mut info = WidgetInfo::labeled(Role::Slider, enabled, &label);
                info.current_text_value = Some(text.clone());
                info
            });
            let chroma = value.chroma;
            ui.ctx().accesskit_node_builder(response.id, |node| {
                use egui::accesskit::Action;
                node.set_numeric_value(f64::from(chroma));
                node.set_min_numeric_value(0.0);
                node.set_max_numeric_value(1.0);
                node.set_numeric_value_step(f64::from(step));
                node.add_action(Action::Increment);
                node.add_action(Action::Decrement);
            });
            let slider = luminance.map(|range| {
                let name = strings::tr_args(
                    ui.ctx(),
                    &crate::strings::WHEEL_LUMINANCE,
                    &[("wheel", &label)],
                );
                luminance_slider(
                    ui,
                    &name,
                    range,
                    mode,
                    default,
                    value,
                    &mut hook,
                    &mut changes,
                )
            });
            (response, slider)
        });
        let (response, luminance) = inner.inner;
        WheelOutput {
            response,
            luminance,
            changes,
        }
    }
}

/// The disc's gestures, keys and assistive actions.
#[allow(clippy::too_many_arguments)]
fn disc(
    ui: &mut Ui,
    response: &Response,
    center: Pos2,
    radius: f32,
    mode: WheelMode,
    default: WheelValue,
    step: f32,
    value: &mut WheelValue,
    hook: &mut Option<Hook<'_, WheelValue>>,
    changes: &mut Vec<Change<WheelValue>>,
) {
    let grab_id = GRAB.keyed(response.id);
    let mut grab = grab_id.get(ui.ctx());
    let mut propose = |value: &mut WheelValue, to: WheelValue, cause: Cause, done: bool| {
        if to == *value {
            return;
        }
        let c = Change {
            from: *value,
            to,
            cause,
            done,
        };
        if let Some(c) = change::decide(hook, c) {
            *value = c.to;
            changes.push(c);
        }
    };
    let unit = |pos: Pos2| {
        let r = radius.max(1.0);
        [(pos.x - center.x) / r, (center.y - pos.y) / r]
    };
    let pressed = ui.input(|i| i.pointer.primary_pressed()) && response.is_pointer_button_down_on();
    if pressed {
        let at = match (mode, response.interact_pointer_pos()) {
            (WheelMode::Picker, Some(pos)) => unit(pos),
            _ => value.xy(),
        };
        grab = Some(Grab {
            start: *value,
            at,
            cause: if mode == WheelMode::Picker {
                Cause::Click
            } else {
                Cause::Drag
            },
        });
        if mode == WheelMode::Picker {
            let to = value.with_xy(at);
            propose(value, to, Cause::Click, false);
        }
    }
    match change::gesture(ui, response) {
        Gesture::None => {}
        Gesture::Moved(d) => {
            if let Some(g) = grab.as_mut() {
                let at = match (mode, response.interact_pointer_pos()) {
                    (WheelMode::Picker, Some(pos)) => unit(pos),
                    _ => {
                        let r = radius.max(1.0);
                        let p = [g.at[0] + d.x / r, g.at[1] - d.y / r];
                        let len = (p[0] * p[0] + p[1] * p[1]).sqrt();
                        if len > 1.0 {
                            [p[0] / len, p[1] / len]
                        } else {
                            p
                        }
                    }
                };
                g.at = at;
                g.cause = Cause::Drag;
                let to = value.with_xy(at);
                propose(value, to, Cause::Drag, false);
            }
        }
        Gesture::Ended => {
            if let Some(g) = grab.take() {
                finish(g, value, &mut propose);
            }
        }
        Gesture::Cancelled => {
            if let Some(g) = grab.take() {
                *value = g.start;
            }
        }
    }
    if response.clicked()
        && let Some(g) = grab.take()
    {
        finish(g, value, &mut propose);
    }
    if response.double_clicked() {
        grab = None;
        propose(value, default, Cause::Reset, true);
    }
    if response.has_focus() {
        change::lock_arrows(ui, response.id);
        let [x, y] = change::arrows(ui);
        if x != 0.0 || y != 0.0 {
            let s = step * change::scale(ui);
            let [px, py] = value.xy();
            let to = value.with_xy([px + x * s, py + y * s]);
            propose(value, to, Cause::Key, true);
        }
    }
    let steps = change::access_steps(ui, response.id);
    if steps != 0.0 {
        let to = WheelValue {
            chroma: (value.chroma + steps * step).clamp(0.0, 1.0),
            ..*value
        };
        propose(value, to, Cause::Access, true);
    }
    match grab {
        Some(g) => grab_id.set(ui.ctx(), g),
        None => {
            grab_id.remove(ui.ctx());
        }
    }
}

/// A gesture's end: one done change from the value before it (a veto
/// leaves that value).
fn finish(
    grab: Grab,
    value: &mut WheelValue,
    propose: &mut impl FnMut(&mut WheelValue, WheelValue, Cause, bool),
) {
    if grab.start == *value {
        return;
    }
    let end = *value;
    *value = grab.start;
    propose(value, end, grab.cause, true);
}

/// The luminance slider, its changes going through the wheel's hook.
#[allow(clippy::too_many_arguments)]
fn luminance_slider(
    ui: &mut Ui,
    name: &str,
    range: RangeInclusive<f64>,
    mode: WheelMode,
    default: WheelValue,
    value: &mut WheelValue,
    hook: &mut Option<Hook<'_, WheelValue>>,
    changes: &mut Vec<Change<WheelValue>>,
) -> Response {
    let base = *value;
    let mut lum = f64::from(value.luminance);
    let mut slider = Slider::new(&mut lum, name, range)
        .step(0.01)
        .default(f64::from(default.luminance))
        .on_change(|c: &slider::Change| {
            let at = |l: f64| WheelValue {
                luminance: l as f32,
                ..base
            };
            let proposed = Change {
                from: at(c.from),
                to: at(c.to),
                cause: Cause::of_slider(c.cause),
                done: c.done,
            };
            match change::decide(hook, proposed) {
                Some(written) => {
                    let to = f64::from(written.to.luminance);
                    let redirected = written.to.luminance != at(c.to).luminance;
                    changes.push(written);
                    if redirected {
                        slider::Verdict::Redirect(to)
                    } else {
                        slider::Verdict::Go
                    }
                }
                None => slider::Verdict::Veto,
            }
        });
    if mode == WheelMode::Offset {
        slider = slider.signed();
    }
    let response = slider.show(ui).response;
    value.luminance = lum as f32;
    response
}

/// The disc: the colours, the ring, the centre, the puck, the focus.
fn paint(
    ui: &Ui,
    response: &Response,
    center: Pos2,
    radius: f32,
    mode: WheelMode,
    value: WheelValue,
) {
    if !ui.is_rect_visible(response.rect) {
        return;
    }
    let t = WheelTokens::of(ui.ctx());
    let tokens = Tokens::of(ui.ctx());
    let painter = ui.painter();
    // A fan of triangles: the hue around the rim, towards the centre's
    // colour (white for a picker, grey for a grading wheel). The
    // colours are the content: not tokens.
    let (middle, rim_s, rim_v) = match mode {
        WheelMode::Picker => (Color32::WHITE, 1.0, 1.0),
        WheelMode::Offset => (Color32::from_gray(128), 0.55, 0.8),
    };
    let segments = 96;
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: center,
        uv: WHITE_UV,
        color: middle,
    });
    for i in 0..=segments {
        let turn = i as f32 / segments as f32;
        let a = turn * std::f32::consts::TAU;
        mesh.vertices.push(Vertex {
            pos: center + radius * vec2(a.cos(), -a.sin()),
            uv: WHITE_UV,
            color: Hsva::new(turn, rim_s, rim_v, 1.0).into(),
        });
        if i > 0 {
            mesh.add_triangle(0, i as u32, i as u32 + 1);
        }
    }
    painter.add(Shape::mesh(mesh));
    painter.circle_stroke(center, radius, Stroke::new(1.0, t.ring));
    let tick = 4.0;
    painter.line_segment(
        [center - vec2(tick, 0.0), center + vec2(tick, 0.0)],
        Stroke::new(1.0, t.ring),
    );
    painter.line_segment(
        [center - vec2(0.0, tick), center + vec2(0.0, tick)],
        Stroke::new(1.0, t.ring),
    );
    let [x, y] = value.xy();
    let puck = center + radius * vec2(x, -y);
    let fill: Color32 = match mode {
        WheelMode::Picker => Hsva::new(value.hue / 360.0, value.chroma, 1.0, 1.0).into(),
        WheelMode::Offset => t.puck,
    };
    let r = if response.hovered() || response.dragged() {
        6.0
    } else {
        5.0
    };
    painter.circle(puck, r, fill, Stroke::new(2.0, t.puck_outline));
    painter.circle_stroke(puck, r + 1.5, Stroke::new(1.0, t.puck));
    if response.has_focus() {
        painter.circle_stroke(center, radius + 2.0, Stroke::new(2.0, tokens.focus_ring));
    }
}

/// What the wheel shows, for screen readers.
fn value_text(ctx: &egui::Context, v: WheelValue, mode: WheelMode) -> String {
    let luminance = match mode {
        WheelMode::Offset => {
            let n = strings::number(ctx, f64::from(v.luminance), 2);
            if v.luminance > 0.0 {
                format!("+{n}")
            } else {
                n
            }
        }
        WheelMode::Picker => strings::number(ctx, f64::from(v.luminance), 2),
    };
    strings::tr_args(
        ctx,
        &crate::strings::WHEEL_VALUE,
        &[
            ("hue", &strings::number(ctx, f64::from(v.hue), 0)),
            (
                "chroma",
                &strings::number(ctx, f64::from(v.chroma) * 100.0, 0),
            ),
            ("luminance", &luminance),
        ],
    )
}

/// The size a wheel takes with its slider, for layouts.
pub fn wheel_size(ui: &Ui, size: f32, slider: bool) -> Vec2 {
    let h = if slider {
        size + ui.spacing().item_spacing.y + ui.spacing().interact_size.y
    } else {
        size
    };
    vec2(size, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_puck_and_the_value_agree() {
        let v = WheelValue {
            hue: 90.0,
            chroma: 0.5,
            luminance: 0.1,
        };
        let [x, y] = v.xy();
        assert!(x.abs() < 1e-6 && (y - 0.5).abs() < 1e-6);
        let w = WheelValue::default().with_xy([0.0, -2.0]);
        assert_eq!(w.chroma, 1.0, "kept in the disc");
        assert!((w.hue - 270.0).abs() < 1e-4);
        let centre = v.with_xy([0.0, 0.0]);
        assert_eq!(centre.hue, 90.0, "the hue stays at the centre");
        assert_eq!(WheelValue::from_json(&v.to_json()), Some(v));
    }
}
