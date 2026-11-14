//! A number input (a spin box): the value typed in a field, its unit
//! beside it, and small up and down arrows. For a value whose range is
//! too wide for a slider (a count up to millions, days up to years).
//!
//! - Typing commits on Enter or when the field loses focus; Escape
//!   drops the typed text.
//! - A typed value outside the range, a fraction where a whole number is
//!   expected, or a text that is not a number is refused: the value stays
//!   and the reason shows beside the field.
//! - The arrows, the Up and Down keys and the mouse wheel (over the
//!   focused field) step the value, clamped to the range; with Shift, ten
//!   steps at once.
//! - The value is shown in the interface's number format (large counts
//!   grouped: 1 000 000).
//!
//! ```
//! use egui_pop_core::number::NumberInput;
//!
//! fn kept(ui: &mut egui::Ui, files: &mut f64) {
//!     NumberInput::new(files, "Files kept", 1000.0..=100_000_000.0)
//!         .integer()
//!         .unit("files")
//!         .show(ui);
//! }
//! ```

use std::ops::RangeInclusive;

use egui::{
    Align2, Color32, Id, Key, Modifiers, Pos2, Rect, Response, Role, Sense, Shape, Stroke,
    TextEdit, Ui,
};
use egui_pop_model::values::{Descriptor, Type};

use crate::strings::{self, NUMBER_DECREASE, NUMBER_INCREASE, NUMBER_NOT_A_NUMBER};
use crate::strings::{NUMBER_OUT_OF_RANGE, NUMBER_WHOLE};

/// The widest range, `max - min`, still drawn as a slider by
/// [`crate::param`]; past it, a value is a [`NumberInput`]. An image
/// parameter (a radius up to 500, a black point up to 255, a hue in
/// degrees) still slides; a count up to millions or days up to years is
/// typed. A descriptor's soft range, when it has one, is the range
/// measured: its author asked for a slider over it.
pub const SLIDER_SPAN: f64 = 1000.0;

/// Whether a number descriptor is better typed than slid: its range is
/// wider than [`SLIDER_SPAN`].
pub fn is_wide(d: &Descriptor) -> bool {
    matches!(d.ty, Type::Int | Type::Float)
        && d.soft
            .or(d.hard)
            .is_some_and(|(lo, hi)| hi - lo > SLIDER_SPAN)
}

/// Why a typed value was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// The text is not a number.
    NotANumber,
    /// A fraction where a whole number is expected.
    NotWhole,
    /// Outside the range.
    OutOfRange,
}

/// A number input; see the [module](self).
#[derive(Debug)]
#[must_use = "call .show(ui)"]
pub struct NumberInput<'a> {
    value: &'a mut f64,
    label: String,
    range: RangeInclusive<f64>,
    step: f64,
    integer: bool,
    unit: String,
    decimals: usize,
}

/// What the field keeps between frames.
#[derive(Clone, Default)]
struct Typing {
    text: String,
    refused: Option<String>,
    /// Typed in since the last commit: the text is the person's, not
    /// the value's.
    edited: bool,
    /// The value last drawn: one changed elsewhere (a reset) drops
    /// what was typed and its refusal.
    value: Option<f64>,
}

impl<'a> NumberInput<'a> {
    /// A number in `range`, named `label` (its arrows are named after it).
    pub fn new(value: &'a mut f64, label: &str, range: RangeInclusive<f64>) -> Self {
        Self {
            value,
            label: label.into(),
            range,
            step: 1.0,
            integer: false,
            unit: String::new(),
            decimals: 0,
        }
    }

    /// The input a descriptor calls for: its hard range, step, unit and
    /// precision; whole numbers for an int.
    pub fn from_descriptor(value: &'a mut f64, d: &Descriptor) -> Self {
        let (lo, hi) = d.hard.or(d.soft).unwrap_or((f64::MIN, f64::MAX));
        let mut n = Self::new(value, &d.label, lo..=hi)
            .step(d.step)
            .decimals(d.precision as usize);
        if let Some(u) = &d.unit {
            n = n.unit(u);
        }
        if d.ty == Type::Int { n.integer() } else { n }
    }

    /// One arrow step (one by default).
    pub fn step(mut self, step: f64) -> Self {
        if step > 0.0 {
            self.step = step;
        }
        self
    }

    /// Whole numbers only.
    pub fn integer(mut self) -> Self {
        self.integer = true;
        self.step = self.step.max(1.0).round();
        self.decimals = 0;
        self
    }

    /// The unit shown after the field.
    pub fn unit(mut self, unit: &str) -> Self {
        self.unit = unit.into();
        self
    }

    /// Decimals shown.
    pub fn decimals(mut self, decimals: usize) -> Self {
        if !self.integer {
            self.decimals = decimals;
        }
        self
    }

    /// A typed text's value, or why it is refused.
    pub fn parse(&self, text: &str) -> Result<f64, Refusal> {
        let v = crate::slider::parse_typed(text).ok_or(Refusal::NotANumber)?;
        if self.integer && v.fract() != 0.0 {
            return Err(Refusal::NotWhole);
        }
        if !self.range.contains(&v) {
            return Err(Refusal::OutOfRange);
        }
        Ok(v)
    }

    fn clamp(&self, v: f64) -> f64 {
        let v = v.clamp(*self.range.start(), *self.range.end());
        if self.integer { v.round() } else { v }
    }

    fn shown(&self, ctx: &egui::Context, v: f64) -> String {
        strings::number(ctx, v, self.decimals)
    }

    fn reason(&self, ctx: &egui::Context, refusal: &Refusal) -> String {
        match refusal {
            Refusal::NotANumber => strings::tr(ctx, &NUMBER_NOT_A_NUMBER),
            Refusal::NotWhole => strings::tr(ctx, &NUMBER_WHOLE),
            Refusal::OutOfRange => {
                let min = self.shown(ctx, *self.range.start());
                let max = self.shown(ctx, *self.range.end());
                let min = strings::quantity(ctx, &min, &self.unit);
                let max = strings::quantity(ctx, &max, &self.unit);
                strings::tr_args(
                    ctx,
                    &NUMBER_OUT_OF_RANGE,
                    &[("min", min.as_str()), ("max", max.as_str())],
                )
            }
        }
    }

    /// Draws the field, its unit, its arrows and a refusal's reason. The
    /// response is the field's, marked changed when the value changed.
    pub fn show(self, ui: &mut Ui) -> Response {
        ui.horizontal(|ui| self.show_inner(ui)).inner
    }

    fn show_inner(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let id = ui.make_persistent_id(("egui_pop.number", &self.label));
        let edit_id = id.with("field");
        let mut typing: Typing = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        let before = *self.value;
        if typing.value.is_some_and(|v| v != before) {
            typing.edited = false;
            typing.refused = None;
        }
        if !typing.edited && typing.refused.is_none() {
            typing.text = self.shown(&ctx, before);
        }

        let font = egui::TextStyle::Body.resolve(ui.style());
        let widest = self.shown(&ctx, *self.range.end());
        let widest = ui
            .painter()
            .layout_no_wrap(format!("{widest}00"), font, Color32::WHITE)
            .size()
            .x;
        let mut field = ui.add(
            TextEdit::singleline(&mut typing.text)
                .id(edit_id)
                .desired_width(widest.clamp(48.0, 160.0))
                .align(Align2::RIGHT_CENTER),
        );

        if field.changed() {
            typing.edited = true;
        }
        let mut next: Option<f64> = None;
        if field.has_focus() {
            // Shift first: a key without modifiers matches it too.
            let (up10, down10, up, down) = ui.input_mut(|i| {
                (
                    i.count_and_consume_key(Modifiers::SHIFT, Key::ArrowUp),
                    i.count_and_consume_key(Modifiers::SHIFT, Key::ArrowDown),
                    i.count_and_consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.count_and_consume_key(Modifiers::NONE, Key::ArrowDown),
                )
            });
            // The wheel steps the focused field only, so a page scrolled
            // past it never changes it; the panel does not scroll then.
            let wheel = if field.hovered() {
                let w = crate::slider::wheel_steps(ui, field.id);
                ui.input_mut(|i| {
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                    i.events
                        .retain(|e| !matches!(e, egui::Event::MouseWheel { .. }));
                });
                w
            } else {
                0.0
            };
            let steps = up as f64 - down as f64 + 10.0 * (up10 as f64 - down10 as f64) + wheel;
            if steps != 0.0 {
                let from = self.parse(&typing.text).unwrap_or(before);
                next = Some(self.clamp(from + steps * self.step));
            }
        }
        if field.lost_focus() {
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                typing.refused = None;
                typing.edited = false;
            } else {
                match self.parse(&typing.text) {
                    Ok(v) => {
                        typing.refused = None;
                        next = Some(self.clamp(v));
                    }
                    Err(r) => {
                        typing.refused = Some(self.reason(&ctx, &r));
                        typing.edited = false;
                    }
                }
            }
        }

        if !self.unit.is_empty() {
            ui.label(&self.unit);
        }
        let (up, down) = arrows(ui, id, &self.label);
        if up || down {
            let from = if typing.refused.is_some() {
                before
            } else {
                self.parse(&typing.text).unwrap_or(before)
            };
            let sign = if up { 1.0 } else { -1.0 };
            next = Some(self.clamp(from + sign * self.step));
        }

        if let Some(v) = next {
            typing.refused = None;
            typing.edited = false;
            typing.text = self.shown(&ctx, v);
            if v != before {
                *self.value = v;
                field.mark_changed();
            }
        }
        if let Some(reason) = &typing.refused {
            ui.colored_label(ui.visuals().error_fg_color, reason);
        }
        typing.value = Some(*self.value);
        ui.data_mut(|d| d.insert_temp(id, typing));
        field
    }
}

/// The two small arrows, stacked; whether each was clicked.
fn arrows(ui: &mut Ui, id: Id, label: &str) -> (bool, bool) {
    let ctx = ui.ctx().clone();
    let h = ui.spacing().interact_size.y;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(h * 0.6, h), Sense::hover());
    let halves = [
        (
            Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.center().y)),
            NUMBER_INCREASE,
            true,
        ),
        (
            Rect::from_min_max(Pos2::new(rect.min.x, rect.center().y), rect.max),
            NUMBER_DECREASE,
            false,
        ),
    ];
    let mut clicked = [false, false];
    for (i, (half, key, up)) in halves.into_iter().enumerate() {
        let r = ui.interact(half, id.with(("arrow", up)), Sense::click());
        let name = strings::tr_args(&ctx, &key, &[("label", label)]);
        crate::a11y::name(&r, Role::Button, &name);
        let visuals = ui.style().interact(&r);
        let c = half.center();
        let w = half.width() * 0.3;
        let hh = half.height() * 0.25;
        let points = if up {
            vec![
                Pos2::new(c.x - w, c.y + hh),
                Pos2::new(c.x + w, c.y + hh),
                Pos2::new(c.x, c.y - hh),
            ]
        } else {
            vec![
                Pos2::new(c.x - w, c.y - hh),
                Pos2::new(c.x + w, c.y - hh),
                Pos2::new(c.x, c.y + hh),
            ]
        };
        ui.painter().add(Shape::convex_polygon(
            points,
            visuals.fg_stroke.color,
            Stroke::NONE,
        ));
        clicked[i] = r.clicked();
    }
    (clicked[0], clicked[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values_are_checked() {
        let mut v = 30.0;
        let n = NumberInput::new(&mut v, "Days", 0.0..=3650.0).integer();
        assert_eq!(n.parse("90"), Ok(90.0));
        assert_eq!(n.parse("1 000 days"), Ok(1000.0));
        assert_eq!(n.parse("4000"), Err(Refusal::OutOfRange));
        assert_eq!(n.parse("-1"), Err(Refusal::OutOfRange));
        assert_eq!(n.parse("1.5"), Err(Refusal::NotWhole));
        assert_eq!(n.parse("many"), Err(Refusal::NotANumber));
        assert_eq!(n.clamp(5000.0), 3650.0);
    }

    #[test]
    fn only_wide_ranges_are_typed() {
        assert!(is_wide(&Descriptor::int(
            "Files",
            1000,
            100_000_000,
            1_000_000
        )));
        assert!(is_wide(&Descriptor::int("Days", 0, 3650, 30)));
        assert!(!is_wide(&Descriptor::int("Calls", 1, 64, 8)));
        assert!(!is_wide(&Descriptor::int("Radius", 0, 500, 20)));
        assert!(!is_wide(&Descriptor::float("Hue", 0.0, 360.0, 200.0)));
        assert!(!is_wide(
            &Descriptor::int("Files", 0, 100_000, 10).soft(0.0, 100.0)
        ));
        assert!(!is_wide(&Descriptor::float("Gain", 0.0, 2.0, 1.0)));
        assert!(!is_wide(&Descriptor::bool("On", true)));
    }
}
