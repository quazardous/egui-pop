//! The slider family, in the manner of darktable's bauhaus sliders: the
//! label and the value inside the bar, a soft range the bar spans and a
//! hard range a typed value may reach, fine and coarse adjustment, reset
//! on a double click, and a value typed with its unit or as an
//! expression.
//!
//! - [`Slider`]: one value, linear or logarithmic ([`Curve`]), on a plain
//!   or a gradient track ([`Gradient`]: hue, temperature), with an
//!   optional quad button at its end (a colour picker, a toggle).
//! - [`RangeSlider`]: two values on one bar, a low and a high thumb.
//!
//! Gestures:
//!
//! - **Drag** anywhere on the bar: the value follows the pointer's
//!   motion over the soft range; a click never makes it jump. A drag is
//!   one gesture (one undo step when bound to a store); Escape during it
//!   puts the value back with no trace.
//! - **Fine and coarse**: ten times finer and ten times coarser with
//!   the pointer profile's modifiers (`gesture.value.fine` and
//!   `gesture.value.coarse`; Ctrl (Cmd) finer and Shift coarser by
//!   default), for drags, arrows and the wheel alike, unless the
//!   profile gives the arrows their own (`value_keys`: Lightroom's Shift
//!   drag is finer, its Shift with the arrows coarser).
//! - **Keys** when focused: the arrows step, Page Up and Page Down take
//!   coarse steps, Home and End go to the soft ends, Enter opens the
//!   typed entry.
//! - **Wheel**, as the pointer profile says
//!   ([`crate::pointer`]): guarded (the default), it scrolls the panel
//!   and steps a slider only when the slider has the focus or the step
//!   modifiers are held over it (Alt by default), so scrolling past
//!   sliders never changes a value; on values (darktable), it steps the
//!   slider under the pointer, and the panel scrolls with the panel
//!   modifiers (Ctrl+Alt).
//! - **Typed entry**: a number or an expression ([`expr`]: `x * 2`,
//!   `1,5 EV`); units after it are ignored; a value past the soft range
//!   is kept up to the hard one. Enter or leaving the entry (a click
//!   elsewhere, Tab, the focus taken by another widget) commits it;
//!   Escape closes it with nothing. It takes the keyboard when it opens,
//!   and after only when nobody holds the focus for a frame (a panel
//!   redrawn), which does not commit it. While open it owns Escape and
//!   Enter ([`crate::focus::text_input_owns_keyboard`]): key dispatchers
//!   leave them to it.
//! - **Double click**: back to the default; so does a click with the
//!   profile's `gesture.value.reset` modifiers (`Ctrl+PrimaryClick` for
//!   one), which no shipped profile binds.
//! - **Right click** (or the context menu key): a menu, "Type a value"
//!   (the typed entry) and "Reset" when there is a default
//!   ([`Slider::menu`] turns it off; [`request_typed_entry`] opens the
//!   entry from a host's own menu).
//!
//! The keys are [`Gestures`], data a profile may remap. The value text
//! is written in the interface's language ([`crate::strings::number`]),
//! and so is the space before its unit ([`crate::strings::quantity`]).
//!
//! **Hooks.** [`Slider::on_change`] sees every change before it is
//! written, as a [`Change`] (which thumb, from, to, the [`Cause`], and
//! whether the gesture is done), and answers a [`Verdict`]: go, veto or
//! redirect to another value. A drag's frames come with `done: false`;
//! its end comes once more, from the value before the drag to the last
//! one, with `done: true`, and a veto then puts the value back. The
//! application API turns the done changes into the `slider.change` step
//! (`egui_pop_api::sliders`). [`Slider::show`] says what changed.
//!
//! **Accessibility.** The bar is an AccessKit slider named by its label,
//! with its value, its value text (unit included), its range and step,
//! and the increment, decrement and set-value actions.
//!
//! Owned: egui's `Slider` has no soft and hard range, no fine adjustment,
//! no reset, no gradient and no range (see `docs/widgets.md`).
//!
//! ```
//! use egui_pop_core::slider::{Gradient, RangeSlider, Slider};
//!
//! fn sliders(ui: &mut egui::Ui, ev: &mut f64, hue: &mut f64, iso: &mut (f64, f64)) {
//!     ui.add(Slider::new(ev, "Exposure", -3.0..=4.0).hard(-18.0..=18.0).step(0.01).unit("EV").signed().default(0.0));
//!     ui.add(Slider::new(hue, "Hue", 0.0..=360.0).unit("°").gradient(Gradient::hue()));
//!     ui.add(RangeSlider::new(&mut iso.0, &mut iso.1, "ISO", 50.0..=25600.0).logarithmic().integer());
//! }
//! ```

pub mod expr;
mod gradient;
mod range;

pub use gradient::Gradient;
pub use range::RangeSlider;

use std::fmt;
use std::ops::RangeInclusive;

use egui::{
    Color32, Id, Key, Modifiers, Rect, Response, Role, Sense, TextEdit, Ui, Vec2, Widget,
    WidgetInfo, pos2, vec2,
};
use egui_pop_model::pointer::KnobDrag;
use egui_pop_model::values::{Descriptor, Type};

use crate::field::FieldState;
use crate::icon::Icon;
use crate::kit::{BoxStyle, Kit, KitCx, Part, State, Tone};
use crate::state::Slot;
use crate::theme::Tokens;

/// The slider's gestures, as data so a profile can remap them: by
/// default they follow the pointer profile ([`Gestures::from_map`]);
/// [`set_gestures`] sets them outright. The invariants hold whatever
/// the keys: a click never makes the value jump, a drag is relative and
/// one gesture, a modifier scales the step and never changes what is
/// adjusted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gestures {
    /// Held: ten times finer (drag, arrows, wheel); none: never.
    pub fine: Modifiers,
    /// Held: ten times coarser; none: never.
    pub coarse: Modifiers,
    /// Held with the arrow keys: ten times finer; none: never.
    pub key_fine: Modifiers,
    /// Held with the arrow keys: ten times coarser; none: never.
    pub key_coarse: Modifiers,
    /// Held over a slider without the focus: the wheel steps it (else
    /// the wheel scrolls the panel); none: only with
    /// [`Gestures::wheel_steps`] or the focus.
    pub wheel: Modifiers,
    /// The wheel steps the slider under the pointer, focused or not
    /// (darktable); the panel scrolls with [`Gestures::panel`] held.
    pub wheel_steps: bool,
    /// Held: the wheel scrolls the panel, even over a slider; none:
    /// never.
    pub panel: Modifiers,
    /// Opens the typed entry on a focused slider.
    pub typed_entry: Key,
    /// During a drag: puts the value back, no trace.
    pub cancel: Key,
    /// A double click resets to the default.
    pub reset_on_double_click: bool,
    /// Held with a primary click: back to the default
    /// (`gesture.value.reset`, `Ctrl+PrimaryClick` for one); none: a
    /// click never resets, as in every shipped profile.
    pub reset_click: Modifiers,
    /// How a drag turns a knob that does not say
    /// ([`Knob::drag`](crate::knob::Knob::drag)).
    pub knob_drag: KnobDrag,
    /// A knob's drag hides the pointer and puts it back where the drag
    /// started, for a knob that does not say
    /// ([`Knob::hide_pointer`](crate::knob::Knob::hide_pointer)); off
    /// in every shipped profile.
    pub knob_hide_pointer: bool,
}

impl Default for Gestures {
    /// The default pointer profile's.
    fn default() -> Self {
        Self::from_map(&crate::pointer::default_map())
    }
}

impl Gestures {
    /// What a pointer map says: the modifiers of `gesture.value.step`
    /// on the wheel (a plain `Wheel` binding steps unfocused sliders),
    /// of `gesture.value.panel` on the wheel, of `gesture.value.fine`
    /// and `gesture.value.coarse` on a primary drag; the profile's
    /// `value_keys` for the arrows (the drag's where it gives none); the
    /// profile's option for the wheel over sliders.
    pub fn from_map(map: &egui_pop_model::pointer::PointerMap) -> Self {
        use egui_pop_model::keymap::{Button, Pointer};
        use egui_pop_model::pointer::{SCOPE_VALUE, SliderWheel, gesture};
        fn on(
            map: &egui_pop_model::pointer::PointerMap,
            command: &egui_pop_model::ids::CommandId,
            pointer: Pointer,
        ) -> Vec<Modifiers> {
            map.chords(SCOPE_VALUE, command)
                .filter(|c| c.pointer() == Some(pointer))
                .map(crate::pointer::modifiers_of)
                .collect()
        }
        let first = |mods: Vec<Modifiers>| mods.into_iter().find(|m| m.any());
        let step = on(map, &gesture::VALUE_STEP, Pointer::Wheel);
        let drag = Pointer::Drag(Button::Primary);
        let fine = first(on(map, &gesture::VALUE_FINE, drag)).unwrap_or(Modifiers::NONE);
        let coarse = first(on(map, &gesture::VALUE_COARSE, drag)).unwrap_or(Modifiers::NONE);
        let keys = |text: &str, drag: Modifiers| {
            if text.trim().is_empty() {
                drag
            } else {
                crate::pointer::modifiers_from_text(text)
            }
        };
        Self {
            fine,
            coarse,
            key_fine: keys(&map.value_keys.fine, fine),
            key_coarse: keys(&map.value_keys.coarse, coarse),
            wheel: first(step.clone()).unwrap_or(Modifiers::NONE),
            wheel_steps: map.options.slider_wheel == SliderWheel::Value
                || step.iter().any(|m| !m.any()),
            panel: first(on(map, &gesture::VALUE_PANEL, Pointer::Wheel)).unwrap_or(Modifiers::NONE),
            typed_entry: Key::Enter,
            cancel: Key::Escape,
            reset_on_double_click: true,
            reset_click: first(on(
                map,
                &gesture::VALUE_RESET,
                Pointer::Click(Button::Primary),
            ))
            .unwrap_or(Modifiers::NONE),
            knob_drag: map.options.knob_drag,
            knob_hide_pointer: false,
        }
    }
}

const GESTURES: Slot<Gestures> = Slot::new("core.slider.gestures");
/// A drag's position as a fraction of the bar, per slider.
const DRAG_ACC: Slot<f64> = Slot::new("core.slider.drag_acc");
/// The value a drag started from, per slider.
const DRAG_START: Slot<f64> = Slot::new("core.slider.drag_start");
/// The text of an open typed entry, per entry.
const TYPING: Slot<String> = Slot::new("core.slider.typing");
/// The typed entry takes the keyboard on its first frame, per entry.
const TYPING_FOCUS: Slot<bool> = Slot::new("core.slider.typing_focus");
/// The pass a typed entry without the focus was last held open.
const TYPING_HELD: Slot<u64> = Slot::new("core.slider.typing_held");

/// The sliders' gestures in a context: those set, else the pointer
/// profile's ([`crate::pointer::map`]).
pub fn gestures(ctx: &egui::Context) -> Gestures {
    GESTURES
        .get(ctx)
        .unwrap_or_else(|| Gestures::from_map(&crate::pointer::map(ctx)))
}

/// Sets the sliders' gestures in a context, over the pointer profile's.
pub fn set_gestures(ctx: &egui::Context, gestures: Gestures) {
    GESTURES.set(ctx, gestures);
}

/// How the bar maps values to positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    #[default]
    Linear,
    /// Equal ratios take equal lengths (ISO, a radius, a frequency).
    /// Needs a soft range above zero; else the bar is linear.
    Logarithmic,
}

/// What changed a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Drag,
    Key,
    Wheel,
    Typed,
    /// A double click, back to the default.
    Reset,
    /// An assistive technology's action (increment, decrement, set).
    Access,
}

impl Cause {
    /// Its name in step arguments (`drag`).
    pub fn name(self) -> &'static str {
        match self {
            Cause::Drag => "drag",
            Cause::Key => "key",
            Cause::Wheel => "wheel",
            Cause::Typed => "typed",
            Cause::Reset => "reset",
            Cause::Access => "access",
        }
    }
}

/// Which value of a slider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thumb {
    /// A [`Slider`]'s only value.
    Value,
    /// A [`RangeSlider`]'s low end.
    Low,
    /// A [`RangeSlider`]'s high end.
    High,
}

impl Thumb {
    /// Its name in step arguments (`value`, `low`, `high`).
    pub fn name(self) -> &'static str {
        match self {
            Thumb::Value => "value",
            Thumb::Low => "low",
            Thumb::High => "high",
        }
    }
}

/// A change a slider proposes, or made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    pub thumb: Thumb,
    pub from: f64,
    pub to: f64,
    pub cause: Cause,
    /// The gesture is over: every change but a drag's frames. A drag's
    /// end comes as one done change from the value before it.
    pub done: bool,
    /// How far the value moved: `to - from`, except on an endless knob
    /// that wraps ([`Knob::endless`](crate::knob::Knob::endless)), where a
    /// drag, a step or the wheel give the motion unwrapped (a turn past
    /// the end counts in full, `to` being wrapped), for a host that maps
    /// changes to nudges. A drag's end gives the whole drag's.
    pub delta: f64,
}

/// What an [`on_change`](Slider::on_change) hook decides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// Write the value proposed.
    Go,
    /// Write nothing (at a drag's end: back to the value before it).
    Veto,
    /// Write this value instead (kept to the hard range).
    Redirect(f64),
}

/// A hook seeing each change before it is written.
pub(crate) struct Hook<'a>(pub(crate) Box<dyn FnMut(&Change) -> Verdict + 'a>);

impl fmt::Debug for Hook<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Hook")
    }
}

/// The change as the hook leaves it; `None` when vetoed, or when the
/// field's state refuses changes (read-only, disabled).
pub(crate) fn decide(hook: &mut Option<Hook<'_>>, change: Change, spec: &Spec) -> Option<Change> {
    if !spec.state.is_editable() {
        return None;
    }
    match hook.as_mut().map_or(Verdict::Go, |h| (h.0)(&change)) {
        Verdict::Go => Some(change),
        Verdict::Veto => None,
        Verdict::Redirect(v) => {
            let to = spec.settle(v, spec.hard);
            Some(Change {
                to,
                delta: to - change.from,
                ..change
            })
        }
    }
}

/// What a slider did in a frame.
#[derive(Debug)]
pub struct SliderOutput {
    /// The bar (for a range slider, both thumbs' responses merged);
    /// `changed()` when a value changed.
    pub response: Response,
    /// The changes written this frame, after the hook.
    pub changes: Vec<Change>,
    /// The quad button, when there is one.
    pub quad: Option<Response>,
}

impl SliderOutput {
    /// Whether the quad button was clicked.
    pub fn quad_clicked(&self) -> bool {
        self.quad.as_ref().is_some_and(Response::clicked)
    }
}

/// The square button at a slider's end.
#[derive(Debug)]
struct Quad<'a> {
    icon: Icon,
    label: String,
    /// A toggle's state, flipped by a click.
    on: Option<&'a mut bool>,
}

/// What both sliders share: the ranges, steps and how values are shown.
#[derive(Clone, Debug)]
pub(crate) struct Spec {
    pub label: String,
    pub soft: (f64, f64),
    pub hard: (f64, f64),
    pub step: f64,
    pub coarse: f64,
    pub unit: String,
    pub precision: usize,
    pub scale: f64,
    pub integer: bool,
    pub signed: bool,
    pub curve: Curve,
    pub gradient: Option<Gradient>,
    /// The field's state ([`crate::field`]).
    pub state: FieldState,
}

impl Spec {
    pub(crate) fn new(label: &str, soft: RangeInclusive<f64>) -> Self {
        let (lo, hi) = (*soft.start(), *soft.end());
        let step = if hi > lo { (hi - lo) / 100.0 } else { 0.01 };
        Self {
            label: label.into(),
            soft: (lo, hi),
            hard: (lo, hi),
            step,
            coarse: step * 10.0,
            unit: String::new(),
            precision: precision_of(step),
            scale: 1.0,
            integer: false,
            signed: false,
            curve: Curve::Linear,
            gradient: None,
            state: FieldState::default(),
        }
    }

    fn from_descriptor(d: &Descriptor) -> Self {
        let hard = d.hard.unwrap_or((0.0, 1.0));
        let soft = d.soft.unwrap_or(hard);
        let mut s = Self::new(&d.label, soft.0..=soft.1);
        s.set_hard(hard.0..=hard.1);
        s.set_step(d.step);
        if d.coarse > 0.0 {
            s.coarse = d.coarse;
        }
        s.precision = d.precision as usize;
        s.scale = d.display_scale;
        if let Some(u) = &d.unit {
            s.unit = u.clone();
        }
        s.integer = d.ty == Type::Int;
        s
    }

    pub(crate) fn set_hard(&mut self, hard: RangeInclusive<f64>) {
        self.hard = (hard.start().min(self.soft.0), hard.end().max(self.soft.1));
    }

    pub(crate) fn set_step(&mut self, step: f64) {
        if step > 0.0 {
            self.step = step;
            self.coarse = step * 10.0;
            self.precision = precision_of(step);
        }
    }

    fn log(&self) -> bool {
        self.curve == Curve::Logarithmic && self.soft.0 > 0.0 && self.soft.1 > self.soft.0
    }

    /// Where a value sits on the bar: 0 at the soft start, 1 at its end
    /// (outside for a value past the soft range).
    pub(crate) fn frac(&self, v: f64) -> f64 {
        let (lo, hi) = self.soft;
        if self.log() {
            (v.max(f64::MIN_POSITIVE).ln() - lo.ln()) / (hi.ln() - lo.ln())
        } else if hi > lo {
            (v - lo) / (hi - lo)
        } else {
            0.0
        }
    }

    /// The value at a place on the bar.
    pub(crate) fn value_at(&self, frac: f64) -> f64 {
        let (lo, hi) = self.soft;
        if self.log() {
            (lo.ln() + frac * (hi.ln() - lo.ln())).exp()
        } else {
            lo + frac * (hi - lo)
        }
    }

    /// `v` moved by `steps` steps, `scale` being the modifiers' factor
    /// (0.1 fine, 10 coarse). A logarithmic bar steps by a hundredth of
    /// its length.
    pub(crate) fn stepped(&self, v: f64, steps: f64, scale: f64) -> f64 {
        if steps == 0.0 {
            return v;
        }
        if self.log() {
            return self.value_at(self.frac(v) + steps * 0.01 * scale);
        }
        let step = self.step * scale.min(self.coarse / self.step);
        v + steps * step
    }

    pub(crate) fn settle(&self, v: f64, (lo, hi): (f64, f64)) -> f64 {
        let v = if self.integer { v.round() } else { v };
        v.clamp(lo, hi)
    }

    /// The number shown, in the interface's language, without the unit.
    pub(crate) fn number(&self, ctx: &egui::Context, v: f64) -> String {
        let shown = v * self.scale;
        let n = crate::strings::number(ctx, shown, self.precision);
        let nonzero = n.chars().any(|c| c.is_ascii_digit() && c != '0');
        match n.strip_prefix('-') {
            // Never "-0.00".
            Some(rest) if !nonzero => rest.to_string(),
            _ if self.signed && nonzero && shown > 0.0 => format!("+{n}"),
            _ => n,
        }
    }

    /// The number with its unit.
    pub(crate) fn text(&self, ctx: &egui::Context, v: f64) -> String {
        self.with_unit(ctx, &self.number(ctx, v))
    }

    /// A number written with the unit after it, spaced as the
    /// interface's language does ([`crate::strings::quantity`]).
    pub(crate) fn with_unit(&self, ctx: &egui::Context, n: &str) -> String {
        crate::strings::quantity(ctx, n, &self.unit)
    }

    /// A typed text's value (in the slider's units), `x` being `current`.
    pub(crate) fn parse(&self, text: &str, current: f64) -> Option<f64> {
        expr::eval(text, current * self.scale).map(|v| v / self.scale)
    }
}

fn precision_of(step: f64) -> usize {
    if step >= 1.0 || step <= 0.0 {
        0
    } else {
        (-step.log10()).ceil() as usize
    }
}

/// The builder methods both sliders share, over their `spec`.
macro_rules! spec_builders {
    () => {
        /// The range a typed value may reach (it always holds the soft
        /// range).
        pub fn hard(mut self, hard: std::ops::RangeInclusive<f64>) -> Self {
            self.spec.set_hard(hard);
            self
        }

        /// The step of arrows and the wheel; also sets the precision.
        pub fn step(mut self, step: f64) -> Self {
            self.spec.set_step(step);
            self
        }

        /// The coarse step (Shift, Page Up and Page Down).
        pub fn coarse(mut self, coarse: f64) -> Self {
            if coarse > 0.0 {
                self.spec.coarse = coarse;
            }
            self
        }

        /// Shown after the value (`EV`, `%`).
        pub fn unit(mut self, unit: &str) -> Self {
            self.spec.unit = unit.into();
            self
        }

        /// Digits after the decimal separator.
        pub fn precision(mut self, decimals: usize) -> Self {
            self.spec.precision = decimals;
            self
        }

        /// Shown and typed as `value * scale` (a fraction as a
        /// percentage: 100).
        pub fn display_scale(mut self, scale: f64) -> Self {
            if scale != 0.0 {
                self.spec.scale = scale;
            }
            self
        }

        /// Whole numbers only.
        pub fn integer(mut self) -> Self {
            self.spec.integer = true;
            self.spec.precision = 0;
            self
        }

        /// A plus sign before positive values (`+1.50 EV`).
        pub fn signed(mut self) -> Self {
            self.spec.signed = true;
            self
        }

        /// A logarithmic bar ([`Curve::Logarithmic`](crate::slider::Curve)).
        pub fn logarithmic(mut self) -> Self {
            self.spec.curve = $crate::slider::Curve::Logarithmic;
            self
        }

        /// The track painted with a gradient (hue, temperature) along
        /// the bar.
        pub fn gradient(mut self, gradient: $crate::slider::Gradient) -> Self {
            self.spec.gradient = Some(gradient);
            self
        }

        /// The field's state, drawn and announced as every field's
        /// ([`crate::field`]): a read-only slider takes the focus and
        /// refuses changes, a disabled one is greyed and skipped by Tab,
        /// with its reason as the tooltip.
        pub fn state(mut self, state: $crate::field::FieldState) -> Self {
            self.spec.state = state;
            self
        }
    };
}
pub(crate) use spec_builders;

/// A slider over an `f64`.
#[derive(Debug)]
pub struct Slider<'a> {
    value: &'a mut f64,
    spec: Spec,
    default: Option<f64>,
    hook: Option<Hook<'a>>,
    quad: Option<Quad<'a>>,
    vertical: bool,
    size: Option<Vec2>,
    compact: bool,
    menu: bool,
}

/// The least width a horizontal slider takes, whatever its room: below
/// it the bar is clipped. In a narrow room the label goes first (into
/// the tooltip), then the unit ([`Slider::compact`]).
pub const MIN_WIDTH: f32 = 24.0;

/// A vertical slider's length when no size is given.
pub const VERTICAL_LENGTH: f32 = 160.0;

/// Parses a typed number or expression ([`expr::eval`], with no current
/// value): units after it are ignored, a comma is a point.
pub fn parse_typed(text: &str) -> Option<f64> {
    expr::eval(text, f64::NAN)
}

impl<'a> Slider<'a> {
    /// A slider spanning `soft`, which is also its hard range until
    /// [`Slider::hard`] widens it.
    pub fn new(value: &'a mut f64, label: &str, soft: RangeInclusive<f64>) -> Self {
        Self {
            value,
            spec: Spec::new(label, soft),
            default: None,
            hook: None,
            quad: None,
            vertical: false,
            size: None,
            compact: false,
            menu: true,
        }
    }

    /// From a value descriptor: its label, ranges, steps, unit,
    /// precision, display scale and default.
    pub fn from_descriptor(value: &'a mut f64, d: &Descriptor) -> Self {
        Self {
            value,
            spec: Spec::from_descriptor(d),
            default: d.default.as_f64(),
            hook: None,
            quad: None,
            vertical: false,
            size: None,
            compact: false,
            menu: true,
        }
    }

    spec_builders!();

    /// What a double click goes back to.
    pub fn default(mut self, default: f64) -> Self {
        self.default = Some(default);
        self
    }

    /// A vertical slider, as a mixer's fader: up is more. The same
    /// gestures along the other axis (a drag up does what a drag right
    /// does on a horizontal bar; Up and Right step up), the value at the
    /// top, the label at the bottom, under the track. It has no quad
    /// button.
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    /// Its size, in points: the room it fills (a slot). By default a
    /// horizontal slider takes the width available (at least
    /// [`MIN_WIDTH`]) and a row's height; a vertical one is
    /// [`VERTICAL_LENGTH`] high and a row and a half wide.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// The compact representation: the value alone, the label in the
    /// tooltip (and still the bar's name for assistive technologies). A
    /// slider too narrow for its label and value turns compact by
    /// itself, and drops the unit when even the value does not fit.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// Whether a right click (or the context menu key) opens the value's
    /// menu: "Type a value", and "Reset" with a default. On by default;
    /// off for a host that gives the slider a menu of its own (its
    /// "Type a value" item calls [`request_typed_entry`]).
    pub fn menu(mut self, on: bool) -> Self {
        self.menu = on;
        self
    }

    /// Sees every change before it is written (module docs, "Hooks").
    pub fn on_change(mut self, hook: impl FnMut(&Change) -> Verdict + 'a) -> Self {
        self.hook = Some(Hook(Box::new(hook)));
        self
    }

    /// A quad button at the end of the bar (a colour picker): its click
    /// is in [`SliderOutput::quad_clicked`]. `label` names it.
    pub fn quad(mut self, icon: Icon, label: &str) -> Self {
        self.quad = Some(Quad {
            icon,
            label: label.into(),
            on: None,
        });
        self
    }

    /// A quad button toggling `on` (an effect on or off), lit when on.
    pub fn quad_toggle(mut self, icon: Icon, label: &str, on: &'a mut bool) -> Self {
        self.quad = Some(Quad {
            icon,
            label: label.into(),
            on: Some(on),
        });
        self
    }

    /// Draws the slider; says what changed.
    pub fn show(self, ui: &mut Ui) -> SliderOutput {
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

    fn draw(self, ui: &mut Ui) -> SliderOutput {
        let Slider {
            value,
            spec,
            default,
            mut hook,
            quad,
            vertical,
            size,
            compact,
            menu,
        } = self;
        let row_height = ui.spacing().interact_size.y;
        let wanted = match (size, vertical) {
            (Some(s), false) => vec2(s.x.max(MIN_WIDTH), s.y.max(1.0)),
            (Some(s), true) => vec2(s.x.max(1.0), s.y.max(MIN_WIDTH)),
            (None, false) => vec2(ui.available_width().max(MIN_WIDTH), row_height),
            (None, true) => vec2(row_height * 1.5, VERTICAL_LENGTH),
        };
        let (id, row) = ui.allocate_space(wanted);
        let quad = if vertical { None } else { quad };
        let (bar, quad_rect) = split_quad(row, quad.is_some(), ui);
        let mut response = ui.interact(bar, id, Sense::click_and_drag());
        let quad = quad.map(|q| show_quad(ui, quad_rect, id, q));
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
            delta: to - from,
        };
        let typing_id = typing_id(id);
        if take_typed_request(ui.ctx(), id) && spec.state.is_editable() {
            open_typed_entry(ui, typing_id, spec.number(ui.ctx(), *value));
        }
        let mut label_shown = true;
        if let Some(text) = typing_text(ui.ctx(), typing_id) {
            if let Some(end) = typed_entry(ui, bar, typing_id, text, &spec, *value) {
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
            let scale = modifier_scale(ui, &g);
            let along = if vertical {
                Axis::Vertical(bar.height())
            } else {
                Axis::Horizontal(bar.width())
            };
            match drag_along(ui, &response, &spec, *value, spec.soft, scale, along) {
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
            if reset_clicked(ui, &response, &g)
                && let Some(d) = default
                && d != *value
            {
                apply(change(*value, d, Cause::Reset, true), value);
            }
            if response.clicked() {
                // A click focuses; it never makes the value jump.
                response.request_focus();
            }
            if menu
                && value_menu(&response, &spec, default.filter(|d| *d != *value).is_some())
                && let Some(d) = default
            {
                apply(change(*value, d, Cause::Reset, true), value);
            }
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
                open_typed_entry(ui, typing_id, spec.number(ui.ctx(), *value));
            }
            let text = spec.text(ui.ctx(), *value);
            let past = *value < spec.soft.0 || *value > spec.soft.1;
            label_shown = if vertical {
                paint_vertical(ui, bar, &response, &spec, *value, past)
            } else {
                paint_bar_fitted(
                    ui,
                    bar,
                    &[&response],
                    &spec,
                    &[*value],
                    &text,
                    past,
                    compact,
                )
            };
            if label_shown {
                paint_state(ui, bar, &spec);
            }
        }
        if *value != before {
            response.mark_changed();
        }
        let enabled = ui.is_enabled();
        let v = *value;
        response.widget_info(|| WidgetInfo::slider(enabled, v, &spec.label));
        access(ui, response.id, &spec, v, spec.soft);
        if vertical {
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_orientation(egui::accesskit::Orientation::Vertical);
            });
        }
        state_of(ui, &[&response], &spec, "slider");
        let mut response = crate::field::hover(ui, &response, &spec.state);
        if !label_shown {
            // Compact: the label is read in the tooltip.
            response = response.on_hover_text(&spec.label);
        }
        SliderOutput {
            response,
            changes,
            quad,
        }
    }
}

impl Widget for Slider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

/// Asks the slider or knob whose response is `widget` to open its
/// typed entry, with its value, in its next pass: what the "Type a
/// value" item of its menu does, for a host that gives it a menu of its
/// own ([`Slider::menu`]).
pub fn request_typed_entry(ctx: &egui::Context, widget: Id) {
    TYPE_REQUEST.keyed(widget).set(ctx, true);
    ctx.request_repaint();
}

/// A typed entry asked for, per widget ([`request_typed_entry`]).
const TYPE_REQUEST: Slot<bool> = Slot::new("core.slider.type_request");

/// Whether a typed entry was asked for `widget`; the request is taken.
pub(crate) fn take_typed_request(ctx: &egui::Context, widget: Id) -> bool {
    let slot = TYPE_REQUEST.keyed(widget);
    let asked = slot.get(ctx).unwrap_or(false);
    if asked {
        slot.remove(ctx);
    }
    asked
}

/// Whether this pass's click is the profile's reset click
/// ([`Gestures::reset_click`]).
pub(crate) fn reset_clicked(ui: &Ui, response: &Response, g: &Gestures) -> bool {
    g.reset_click.any()
        && response.clicked()
        && ui.input(|i| i.modifiers.matches_logically(g.reset_click))
}

/// A value's menu, opened by a right click or the context menu key:
/// "Type a value" (asks the typed entry, [`request_typed_entry`]) and,
/// with `reset`, "Reset". Whether "Reset" was chosen.
pub(crate) fn value_menu(response: &Response, spec: &Spec, reset: bool) -> bool {
    let editable = spec.state.is_editable();
    let mut chosen = false;
    crate::pointer::claim::context_menu(response, |ui| {
        let ctx = ui.ctx().clone();
        if ui
            .add_enabled(
                editable,
                egui::Button::new(crate::strings::tr(&ctx, &crate::strings::SLIDER_TYPE_VALUE)),
            )
            .clicked()
        {
            request_typed_entry(&ctx, response.id);
            ui.close();
        }
        if reset
            && ui
                .add_enabled(
                    editable,
                    egui::Button::new(crate::strings::tr(&ctx, &crate::strings::RESET)),
                )
                .clicked()
        {
            chosen = true;
            ui.close();
        }
    });
    chosen
}

/// The text of a slider's open typed entry (`typing_id`'s).
pub(crate) fn typing_text(ctx: &egui::Context, typing_id: Id) -> Option<String> {
    TYPING.keyed(typing_id).get(ctx)
}

/// The id of a slider's typed entry (for tests).
pub fn typing_id(slider: Id) -> Id {
    slider.with("egui_pop_slider_typing")
}

/// The bar and, when there is a quad, the square at its end.
fn split_quad(row: Rect, quad: bool, ui: &Ui) -> (Rect, Rect) {
    if !quad {
        return (row, Rect::NOTHING);
    }
    let side = row.height();
    let gap = ui.spacing().item_spacing.x.min(4.0);
    let bar = Rect::from_min_max(row.min, pos2(row.right() - side - gap, row.bottom()));
    let quad = Rect::from_min_max(pos2(row.right() - side, row.top()), row.max);
    (bar, quad)
}

fn show_quad(ui: &mut Ui, rect: Rect, id: Id, mut quad: Quad<'_>) -> Response {
    let mut r = ui.interact(rect, id.with("egui_pop_slider_quad"), Sense::click());
    if r.clicked()
        && let Some(on) = quad.on.as_deref_mut()
    {
        *on = !*on;
        r.mark_changed();
    }
    let enabled = ui.is_enabled();
    let state = quad.on.as_deref().copied();
    let label = quad.label.as_str();
    match state {
        Some(s) => r.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, s, label)),
        None => r.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label)),
    }
    if ui.is_rect_visible(rect) {
        let kit = Kit::of(ui.ctx());
        let toggle = &kit.slider_toggle;
        let part = Part {
            rect,
            room: rect,
            state: State::from_response(&r).with(State::SELECTED, state == Some(true)),
            label: None,
        };
        let painter = clipped(ui, rect, toggle.style(part.state));
        let cx = KitCx {
            painter: &painter,
            ppp: ui.ctx().pixels_per_point(),
        };
        toggle.paint(&cx, &part);
        quad.icon.paint(
            &painter,
            rect.shrink(rect.height() * 0.2),
            toggle.style(part.state).text,
        );
    }
    r
}

/// A painter clipped to a part, and to its focus ring when the ring
/// goes outside it.
fn clipped(ui: &Ui, rect: Rect, b: &BoxStyle) -> egui::Painter {
    let ring = if b.ring_width > 0.0 {
        (b.ring_offset + b.ring_width).max(0.0)
    } else {
        0.0
    };
    ui.painter_at(rect.expand(ring))
}

/// The modifiers' factor on a step: 0.1 fine, 10 coarse, else 1.
pub(crate) fn modifier_scale(ui: &Ui, g: &Gestures) -> f64 {
    scale_of(ui, g.fine, g.coarse)
}

/// The arrow keys' factor on a step, from their own modifiers.
fn key_scale(ui: &Ui, g: &Gestures) -> f64 {
    scale_of(ui, g.key_fine, g.key_coarse)
}

fn scale_of(ui: &Ui, fine: Modifiers, coarse: Modifiers) -> f64 {
    let mods = ui.input(|i| i.modifiers);
    let held = |m: Modifiers| m.any() && mods.matches_logically(m);
    if held(fine) {
        0.1
    } else if held(coarse) {
        10.0
    } else {
        1.0
    }
}

/// A drag's effect in a frame.
pub(crate) enum Drag {
    None,
    /// The value the pointer's motion leads to.
    Moved(f64),
    /// The drag ended; the value before it.
    Ended(f64),
    /// The cancel key ended it; the value before it.
    Cancelled(f64),
}

/// The axis a bar lies along, and its length in points.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Axis {
    Horizontal(f32),
    /// Up is more.
    Vertical(f32),
}

/// A relative drag on `response`: the pointer's motion over `width`
/// (the bar's) moves the value over the soft range, within `bounds` (unless
/// already past them). The motion accumulates exactly, in bar
/// positions; only what is written is rounded.
pub(crate) fn drag(
    ui: &Ui,
    response: &Response,
    spec: &Spec,
    value: f64,
    bounds: (f64, f64),
    scale: f64,
    width: f32,
) -> Drag {
    drag_along(
        ui,
        response,
        spec,
        value,
        bounds,
        scale,
        Axis::Horizontal(width),
    )
}

/// [`drag`] along either axis: up is more on a vertical bar.
pub(crate) fn drag_along(
    ui: &Ui,
    response: &Response,
    spec: &Spec,
    value: f64,
    bounds: (f64, f64),
    scale: f64,
    axis: Axis,
) -> Drag {
    let acc = DRAG_ACC.keyed(response.id);
    let start = DRAG_START.keyed(response.id);
    crate::pointer::drag_cursor(response, crate::pointer::DragKind::Value);
    if response.drag_started() {
        acc.set(ui.ctx(), spec.frac(value));
        start.set(ui.ctx(), value);
    }
    if response.drag_stopped() {
        let start = start.get(ui.ctx()).unwrap_or(value);
        // egui ends a drag on the cancel key.
        let cancel = gestures(ui.ctx()).cancel;
        return if ui.input(|i| i.key_pressed(cancel)) {
            Drag::Cancelled(start)
        } else {
            Drag::Ended(start)
        };
    }
    if response.dragged() {
        let d = response.drag_delta();
        let (moved, length) = match axis {
            Axis::Horizontal(w) => (d.x, w),
            Axis::Vertical(h) => (-d.y, h),
        };
        let delta = f64::from(moved) / f64::from(length.max(1.0)) * scale;
        if delta != 0.0 {
            let slot = acc;
            let acc = slot.get(ui.ctx()).unwrap_or_else(|| spec.frac(value));
            let (lo, hi) = (spec.frac(bounds.0), spec.frac(bounds.1));
            let next = (acc + delta).clamp(lo.min(acc), hi.max(acc));
            slot.set(ui.ctx(), next);
            return Drag::Moved(spec.value_at(next));
        }
    }
    Drag::None
}

/// The keys (when focused), the wheel (when focused or with its
/// modifier) and assistive actions: the value they lead to, kept in
/// `bounds` unless already past them, and what moved it.
pub(crate) fn keys_wheel_access(
    ui: &mut Ui,
    response: &Response,
    spec: &Spec,
    value: f64,
    bounds: (f64, f64),
    scale: f64,
    g: &Gestures,
) -> Option<(f64, Cause)> {
    let (lo, hi) = bounds;
    let mut v = value;
    let mut cause = None;
    if response.has_focus() {
        let key_scale = key_scale(ui, g);
        ui.input(|i| {
            let count = |keys: &[Key]| keys.iter().map(|k| i.num_presses(*k) as f64).sum::<f64>();
            let steps =
                count(&[Key::ArrowRight, Key::ArrowUp]) - count(&[Key::ArrowLeft, Key::ArrowDown]);
            let pages = count(&[Key::PageUp]) - count(&[Key::PageDown]);
            if steps != 0.0 || pages != 0.0 {
                v = spec.stepped(v, steps, key_scale);
                v = spec.stepped(v, pages, 10.0);
                cause = Some(Cause::Key);
            }
            if i.key_pressed(Key::Home) {
                v = lo;
                cause = Some(Cause::Key);
            }
            if i.key_pressed(Key::End) {
                v = hi;
                cause = Some(Cause::Key);
            }
        });
    }
    // The wheel: the panel scrolls, unless the slider has the focus,
    // the wheel modifier is held over it or the profile has the wheel
    // step values; then each notch steps it, a touchpad's stream steps
    // it once a line's length ([`wheel_steps`]), and the panel does not
    // scroll. The panel modifiers always leave the wheel to the panel.
    let mods = ui.input(|i| i.modifiers);
    let wheel_held = g.wheel.any() && mods.matches_logically(g.wheel);
    let panel_held = g.panel.any() && mods.matches_logically(g.panel);
    if response.hovered() && !panel_held && (response.has_focus() || wheel_held || g.wheel_steps) {
        let steps = wheel_steps(ui, response.id);
        if steps != 0.0 {
            v = spec.stepped(v, steps, scale);
            cause = Some(Cause::Wheel);
        }
        // egui spreads a notch's scroll over the next frames: what is
        // left of it is eaten too, every frame the slider holds the
        // wheel, so the panel never drifts after the step.
        ui.input_mut(|i| {
            i.smooth_scroll_delta = egui::Vec2::ZERO;
            i.events
                .retain(|e| !matches!(e, egui::Event::MouseWheel { .. }));
        });
    }
    // Assistive technologies: increment, decrement, set a value.
    let id = response.id;
    let (inc, dec, set) = ui.input(|i| {
        use egui::accesskit::{Action, ActionData};
        let set = i
            .accesskit_action_requests(id, Action::SetValue)
            .filter_map(|r| match r.data {
                Some(ActionData::NumericValue(n)) => Some(n),
                _ => None,
            })
            .last();
        (
            i.num_accesskit_action_requests(id, Action::Increment),
            i.num_accesskit_action_requests(id, Action::Decrement),
            set,
        )
    });
    if inc + dec > 0 {
        v = spec.stepped(v, inc as f64 - dec as f64, 1.0);
        cause = Some(Cause::Access);
    }
    // Keys, the wheel and steps stay in the bounds, unless already past
    // them; a value set is only kept to the hard range.
    if cause.is_some() {
        v = v.clamp(lo.min(value), hi.max(value));
    }
    if let Some(n) = set {
        v = n;
        cause = Some(Cause::Access);
    }
    cause.map(|c| (v, c))
}

/// A touchpad's scroll not yet a whole step, per widget, and when it
/// last came.
const WHEEL_REST: Slot<(f64, f64)> = Slot::new("core.slider.wheel_rest");

/// A pause in a touchpad's stream longer than this, in seconds, drops
/// what was left of a step.
const WHEEL_REST_PAUSE: f64 = 0.4;

/// This pass's wheel input over the widget `id`, in steps: one a notch
/// of a mouse wheel, and a touchpad's points (as the wheel device
/// setting tells them apart) summed over the frames, a step a line's
/// length (`line_scroll_speed`), what is left kept for the next pass.
pub(crate) fn wheel_steps(ui: &Ui, id: Id) -> f64 {
    use egui_pop_model::keymap::Pointer;
    let device = crate::pointer::map(ui.ctx()).wheel_device;
    let line = f64::from(
        ui.ctx()
            .options(|o| o.input_options.line_scroll_speed)
            .max(1.0),
    );
    let (notches, points, now) = ui.input(|i| {
        let mut notches = 0.0;
        let mut points = 0.0;
        for e in &i.events {
            if let egui::Event::MouseWheel { unit, delta, .. } = e {
                let d = delta.y + delta.x;
                match crate::pointer::wheel_pointer(*unit, device) {
                    Pointer::Scroll => {
                        points += f64::from(match unit {
                            egui::MouseWheelUnit::Point => d,
                            egui::MouseWheelUnit::Line => d * line as f32,
                            egui::MouseWheelUnit::Page => d * i.content_rect().height(),
                        });
                    }
                    _ => notches += f64::from(d.signum()),
                }
            }
        }
        (notches, points, i.time)
    });
    if points == 0.0 {
        return notches;
    }
    let slot = WHEEL_REST.keyed(id);
    let rest = match slot.get(ui.ctx()) {
        Some((rest, at)) if now - at <= WHEEL_REST_PAUSE => rest,
        _ => 0.0,
    };
    let total = rest + points / line;
    let whole = total.trunc();
    slot.set(ui.ctx(), (total - whole, now));
    notches + whole
}

/// Opens the typed entry with `text`. The entry takes the keyboard on
/// the frame it first shows, and only then: a click elsewhere keeps it.
pub(crate) fn open_typed_entry(ui: &Ui, typing_id: Id, text: String) {
    TYPING.keyed(typing_id).set(ui.ctx(), text);
    TYPING_FOCUS.keyed(typing_id).set(ui.ctx(), true);
    ui.ctx().request_repaint();
}

/// Takes the auto id an open typed entry takes, so the widgets after a
/// slider keep their ids whether its entry is open or not: a click on
/// one of them while the entry is open must still find it on the next
/// frame (its focus would otherwise go to an id gone with the entry).
pub(crate) fn keep_auto_ids(ui: &mut Ui) {
    ui.skip_ahead_auto_ids(1);
}

/// How a typed entry closed.
pub(crate) struct TypedEnd {
    /// The value typed (`None` when cancelled or not a value).
    pub value: Option<f64>,
    /// Closed by Enter: the keyboard goes back to the slider. Closed by
    /// leaving it (a click elsewhere, Tab), the keyboard stays where it
    /// went.
    pub refocus: bool,
}

/// The typed entry over the bar, open until it closes: Enter or leaving
/// it commits the value typed, Escape closes it with nothing. While open
/// it claims the keyboard ([`crate::focus::claim_keyboard`]), so a key
/// dispatcher leaves Escape and Enter to it.
pub(crate) fn typed_entry(
    ui: &mut Ui,
    bar: Rect,
    typing_id: Id,
    mut text: String,
    spec: &Spec,
    current: f64,
) -> Option<TypedEnd> {
    // Placed over the bar without moving the cursor; it takes one auto
    // id, which a slider without its entry skips (`keep_auto_ids`).
    let edit = ui
        .place(
            bar,
            TextEdit::singleline(&mut text)
                .id(typing_id.with("edit"))
                .align(egui::Align2::CENTER_TOP),
        )
        .accessible_name(crate::strings::tr_args(
            ui.ctx(),
            &crate::strings::SLIDER_ENTRY,
            &[("label", spec.label.as_str())],
        ));
    if TYPING_FOCUS.keyed(typing_id).remove(ui.ctx()).is_some() {
        edit.request_focus();
    }
    // Escape and Enter are the entry's: read as pressed even when a host
    // took them before the entry was drawn, or when the focus plugin
    // swallowed Escape on its way out of the field.
    let (cancel, enter) = ui.input(|i| (pressed(i, Key::Escape), pressed(i, Key::Enter)));
    let cancel = cancel || crate::focus::escape_left(ui.ctx(), edit.id);
    if !cancel && !enter && (edit.has_focus() || held(ui, edit.id, typing_id)) {
        // Open: key dispatchers leave Escape and Enter to it.
        crate::focus::claim_keyboard(ui.ctx(), edit.id);
        TYPING.keyed(typing_id).set(ui.ctx(), text);
        return None;
    }
    TYPING_HELD.keyed(typing_id).remove(ui.ctx());
    TYPING.keyed(typing_id).remove(ui.ctx());
    if cancel || enter {
        // Spent here: nothing drawn after the entry acts on them.
        ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Escape);
            i.consume_key(Modifiers::NONE, Key::Enter);
        });
    }
    Some(TypedEnd {
        value: if cancel {
            None
        } else {
            spec.parse(&text, current)
        },
        refocus: enter && !cancel,
    })
}

/// Whether an entry without the focus stays open: only when nobody took
/// the focus, which a panel redrawn for another writer's change can
/// cause for one frame (the focused field not drawn on a pass drops the
/// focus). The entry then takes the focus back and waits one frame; a
/// real blur commits it: a press or a click elsewhere, Tab, or another
/// widget holding the focus, or the focus still gone on the next frame.
fn held(ui: &Ui, edit: Id, typing_id: Id) -> bool {
    let moved = ui
        .input(|i| i.pointer.any_pressed() || i.pointer.any_released() || i.key_pressed(Key::Tab));
    let elsewhere = ui.memory(|m| m.focused()).is_some_and(|f| f != edit);
    let key = TYPING_HELD.keyed(typing_id);
    let now = ui.ctx().cumulative_pass_nr();
    let held_last = key.get(ui.ctx()).is_some_and(|p| p + 1 == now);
    if moved || elsewhere || held_last {
        return false;
    }
    key.set(ui.ctx(), now);
    ui.memory_mut(|m| m.request_focus(edit));
    ui.ctx().request_repaint();
    true
}

/// Whether `key` was pressed this pass, in the events left or in the
/// input as it came (a host may have consumed it).
fn pressed(i: &egui::InputState, key: Key) -> bool {
    i.key_pressed(key)
        || i.raw
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::Key { key: k, pressed: true, .. } if *k == key))
}

/// The bar, drawn by the kit: the track (`slider`), the fill
/// (`slider-fill`, or the gradient), the thumbs (`slider-thumb`), the
/// label and the value inside (`slider-value`, in the `warning` tone
/// past the soft range). A range's fill runs between its thumbs.
pub(crate) fn paint_bar(
    ui: &Ui,
    rect: Rect,
    responses: &[&Response],
    spec: &Spec,
    thumbs: &[f64],
    text: &str,
    past: bool,
) {
    paint_bar_fitted(ui, rect, responses, spec, thumbs, text, past, false);
}

/// [`paint_bar`] fitted to its width: the label and the value when both
/// fit (and not `compact`), else the value alone, without its unit when
/// even that does not fit. Says whether the label shows.
#[allow(
    clippy::too_many_arguments,
    reason = "paint_bar's arguments and the compact wish"
)]
pub(crate) fn paint_bar_fitted(
    ui: &Ui,
    rect: Rect,
    responses: &[&Response],
    spec: &Spec,
    thumbs: &[f64],
    text: &str,
    past: bool,
    compact: bool,
) -> bool {
    let kit = Kit::of(ui.ctx());
    let font = egui::TextStyle::Button.resolve(ui.style());
    let pad = kit.slider.data.padding_x.max(0.0);
    let measure = |t: &str| {
        ui.painter()
            .layout_no_wrap(t.to_owned(), font.clone(), Color32::PLACEHOLDER)
            .size()
            .x
    };
    let value_w = measure(text);
    let gap = ui.spacing().item_spacing.x;
    let label_shown = !compact && measure(&spec.label) + value_w + gap + 2.0 * pad <= rect.width();
    // Too narrow for the value with its unit: the number alone.
    let number;
    let text = if !label_shown && value_w + 2.0 * pad > rect.width() && !spec.unit.is_empty() {
        number = text
            .strip_suffix(spec.unit.as_str())
            .map_or(text, str::trim_end)
            .to_owned();
        number.as_str()
    } else {
        text
    };
    if !ui.is_rect_visible(rect) {
        return label_shown;
    }
    let tone = if past { Tone::Warning } else { Tone::Neutral };
    // The bar is in the states of every thumb (a range's two).
    let state = responses
        .iter()
        .fold(State::REST, |s, r| s | State::from_response(r))
        .toned(tone);
    let painter = clipped(ui, rect, kit.slider.style(state));
    let cx = KitCx {
        painter: &painter,
        ppp: ui.ctx().pixels_per_point(),
    };
    let label = painter.layout_no_wrap(spec.label.clone(), font.clone(), Color32::PLACEHOLDER);
    let bar = Part {
        rect,
        room: rect,
        state,
        label: label_shown.then_some(&label),
    };
    kit.slider.paint(&cx, &bar);
    let x_of = |v: f64| rect.left() + rect.width() * spec.frac(v).clamp(0.0, 1.0) as f32;
    // The fill is a strip along the bottom, so the label and the value
    // stay on the track's colour (one contrast pair).
    if let Some(g) = &spec.gradient {
        // A gradient's colours are the content: a taller strip.
        let strip = (rect.height() * 0.3).max(4.0);
        let r = Rect::from_min_max(pos2(rect.left(), rect.bottom() - strip), rect.max);
        g.paint(&painter, r);
    } else {
        let fill = &kit.slider_fill;
        let strip = match fill.data.height {
            h if h > 0.0 => h.min(rect.height()),
            _ => (rect.height() * 0.15).max(3.0),
        };
        let (from, to) = match thumbs {
            [v] => (rect.left(), x_of(*v)),
            [a, b, ..] => (x_of(*a), x_of(*b)),
            [] => (rect.left(), rect.left()),
        };
        let strip = Rect::from_min_max(pos2(from, rect.bottom() - strip), pos2(to, rect.bottom()));
        fill.paint_part(
            &cx,
            &Part {
                rect: strip,
                room: rect,
                state,
                label: None,
            },
        );
    }
    let thumb = &kit.slider_thumb;
    let width = thumb.data.width.max(0.0);
    for (i, v) in thumbs.iter().enumerate() {
        let x = x_of(*v);
        let state = responses
            .get(i)
            .map_or(State::REST, |r| State::from_response(r))
            .toned(tone);
        let r = Rect::from_min_max(
            pos2(x - width / 2.0, rect.top()),
            pos2(x + width / 2.0, rect.bottom()),
        );
        thumb.paint_part(
            &cx,
            &Part {
                rect: r,
                room: rect,
                state,
                label: None,
            },
        );
    }
    kit.slider.paint_label(&cx, &bar);
    // The value's digits line up from one value to the next
    // (`font.numeric`): it does not jitter while dragged.
    let value = crate::fonts::number_galley(ui.ctx(), text, font.size, Color32::PLACEHOLDER);
    kit.slider_value.paint_part(
        &cx,
        &Part {
            rect,
            room: rect,
            state,
            label: Some(&value),
        },
    );
    label_shown
}

/// A vertical bar: the track drawn by the kit (`slider`), the fill
/// (`slider-fill`) rising from the bottom, the thumb across it
/// (`slider-thumb`), the value at the top and the label at the bottom,
/// each cut to the width. Says whether the label shows whole.
fn paint_vertical(
    ui: &Ui,
    rect: Rect,
    response: &Response,
    spec: &Spec,
    v: f64,
    past: bool,
) -> bool {
    let kit = Kit::of(ui.ctx());
    let font = egui::TextStyle::Button.resolve(ui.style());
    let tone = if past { Tone::Warning } else { Tone::Neutral };
    let state = State::from_response(response).toned(tone);
    let style = kit.slider.style(state);
    let line = ui.fonts_mut(|f| f.row_height(&font));
    let pad = 2.0;
    let fit = |text: String, color: Color32| {
        let mut job = egui::text::LayoutJob::simple_singleline(text, font.clone(), color);
        job.wrap = egui::text::TextWrapping::truncate_at_width((rect.width() - 2.0 * pad).max(1.0));
        ui.painter().layout_job(job)
    };
    let label = fit(spec.label.clone(), style.text);
    let label_shown = !label.elided;
    if !ui.is_rect_visible(rect) {
        return label_shown;
    }
    let full = spec.text(ui.ctx(), v);
    let mut value = fit(full.clone(), style.text);
    if value.elided {
        value = fit(spec.number(ui.ctx(), v), style.text);
    }
    let value_band = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + line + pad));
    let label_band = Rect::from_min_max(pos2(rect.left(), rect.bottom() - line - pad), rect.max);
    let track = Rect::from_min_max(
        pos2(rect.left(), value_band.bottom()),
        pos2(rect.right(), label_band.top()),
    );
    let painter = clipped(ui, rect, style);
    let cx = KitCx {
        painter: &painter,
        ppp: ui.ctx().pixels_per_point(),
    };
    if track.height() > 0.0 {
        kit.slider.paint(
            &cx,
            &Part {
                rect: track,
                room: track,
                state,
                label: None,
            },
        );
        let y = track.bottom() - track.height() * spec.frac(v).clamp(0.0, 1.0) as f32;
        let strip = match kit.slider_fill.data.height {
            h if h > 0.0 => h.min(track.width()),
            _ => (track.width() * 0.15).max(3.0),
        };
        if let Some(g) = &spec.gradient {
            // A gradient's colours run along the bar: a side strip.
            let r = Rect::from_min_max(
                pos2(track.left(), track.top()),
                pos2(track.left() + strip.max(4.0), track.bottom()),
            );
            g.paint_vertical(&painter, r);
        } else {
            kit.slider_fill.paint_part(
                &cx,
                &Part {
                    rect: Rect::from_min_max(
                        pos2(track.left(), y),
                        pos2(track.left() + strip, track.bottom()),
                    ),
                    room: track,
                    state,
                    label: None,
                },
            );
        }
        // The thumb: a line across the track, in the thumb's colour.
        let thumb = kit.slider_thumb.style(state);
        let color = if thumb.fill == Color32::TRANSPARENT {
            style.text
        } else {
            thumb.fill
        };
        let thickness = kit.slider_thumb.data.width.max(2.0);
        painter.hline(track.x_range(), y, egui::Stroke::new(thickness, color));
    }
    let at = |band: Rect, g: &std::sync::Arc<egui::Galley>| {
        pos2(
            band.center().x - g.size().x / 2.0,
            band.center().y - g.size().y / 2.0,
        )
    };
    painter.galley(at(value_band, &value), value.clone(), style.text);
    painter.galley(at(label_band, &label), label.clone(), style.text);
    label_shown
}

/// Paints the field state on the bar: the edge, and the markers after
/// the label.
pub(crate) fn paint_state(ui: &Ui, bar: Rect, spec: &Spec) {
    if spec.state.is_plain() || !ui.is_rect_visible(bar) {
        return;
    }
    crate::field::paint_edge(ui, bar, &spec.state);
    let t = Tokens::of(ui.ctx());
    let font = egui::TextStyle::Button.resolve(ui.style());
    let label = ui
        .painter()
        .layout_no_wrap(spec.label.clone(), font.clone(), t.text.default)
        .size()
        .x;
    let side = font.size * 0.8;
    crate::field::paint_markers(
        ui,
        pos2(
            bar.left() + Kit::of(ui.ctx()).slider.data.padding_x + label + t.space.xs,
            bar.center().y,
        ),
        side,
        t.space.xs / 2.0,
        &spec.state,
    );
}

/// The field state given to assistive technologies, reported to agents,
/// and the bar's place in the Tab order (one group for a range's two
/// thumbs).
pub(crate) fn state_of(ui: &Ui, responses: &[&Response], spec: &Spec, kind: &str) {
    let group = format!("{:?}", responses.first().map(|r| r.id));
    for r in responses {
        crate::field::access(ui.ctx(), r.id, &spec.state);
        crate::tab::stop(
            ui,
            r,
            crate::tab::TabStop::new()
                .group(group.clone())
                .skip(!spec.state.is_focusable()),
        );
    }
    if let Some(r) = responses.first() {
        crate::field::record(ui.ctx(), r.id, &spec.label, kind, &spec.state);
    }
}

/// The AccessKit node beyond egui's slider info: the range, the step,
/// the value text, and the actions.
pub(crate) fn access(ui: &Ui, id: Id, spec: &Spec, value: f64, (lo, hi): (f64, f64)) {
    let text = spec.text(ui.ctx(), value);
    ui.ctx().accesskit_node_builder(id, |node| {
        use egui::accesskit::Action;
        node.set_min_numeric_value(lo.min(value));
        node.set_max_numeric_value(hi.max(value));
        node.set_numeric_value_step(spec.step);
        node.set_value(text);
        node.add_action(Action::SetValue);
        if value < hi {
            node.add_action(Action::Increment);
        }
        if value > lo {
            node.add_action(Action::Decrement);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{Curve, Spec, parse_typed};

    #[test]
    fn typed_values_keep_the_number_and_drop_the_unit() {
        assert_eq!(parse_typed("1,5 EV"), Some(1.5));
        assert_eq!(parse_typed(" -0.25"), Some(-0.25));
        assert_eq!(parse_typed("+3%"), Some(3.0));
        assert_eq!(parse_typed("EV"), None);
        assert_eq!(parse_typed("x"), None, "no current value");
    }

    #[test]
    fn a_logarithmic_bar_gives_equal_ratios_equal_lengths() {
        let mut s = Spec::new("ISO", 100.0..=6400.0);
        s.curve = Curve::Logarithmic;
        assert!((s.frac(800.0) - 0.5).abs() < 1e-9);
        assert!((s.value_at(0.5) - 800.0).abs() < 1e-6);
        // A hundredth of the bar per step.
        let up = s.stepped(800.0, 1.0, 1.0);
        assert!((s.frac(up) - 0.51).abs() < 1e-9);
        // Not above zero: linear.
        let mut z = Spec::new("Zero", 0.0..=10.0);
        z.curve = Curve::Logarithmic;
        assert!((z.frac(5.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn values_are_shown_signed_and_never_as_minus_zero() {
        let ctx = egui::Context::default();
        let mut s = Spec::new("Exposure", -3.0..=4.0);
        s.precision = 2;
        s.unit = "EV".into();
        assert_eq!(s.text(&ctx, -0.001), "0.00 EV");
        s.signed = true;
        assert_eq!(s.text(&ctx, 1.5), "+1.50 EV");
        assert_eq!(s.text(&ctx, 0.0), "0.00 EV");
        assert_eq!(s.text(&ctx, -1.5), "-1.50 EV");
    }
}
