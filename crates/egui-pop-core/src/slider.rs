//! A slider in the manner of darktable's: the label and the value inside
//! the bar, a soft range the bar spans and a hard range a typed value may
//! reach, fine and coarse adjustment, reset on a double click, and a
//! value typed with its unit.
//!
//! - **Drag** anywhere on the bar: the value follows the pointer's
//!   motion over the soft range (Shift: ten times finer). A drag is one
//!   gesture (one undo step when bound to a store).
//! - **Keys** when focused: the arrows step (Shift: coarse, Ctrl: a
//!   tenth of a step), Home and End go to the soft ends, Enter opens the
//!   typed entry; the **wheel** steps only when the slider has the focus,
//!   so scrolling a panel never changes a value by accident.
//! - **Typed entry**: units after the number are ignored, a comma is a
//!   point; a value past the soft range is kept up to the hard one.
//! - **Double click**: back to the default.
//!
//! Owned: egui's `Slider` has no soft and hard range, no fine adjustment
//! and no reset (see `docs/widgets.md`).
//!
//! ```
//! use egui_pop_core::slider::Slider;
//!
//! fn exposure(ui: &mut egui::Ui, ev: &mut f64) {
//!     ui.add(Slider::new(ev, "Exposure", -3.0..=4.0).hard(-18.0..=18.0).step(0.01).unit("EV").default(0.0));
//! }
//! ```

use std::ops::RangeInclusive;

use egui::{
    Align2, CornerRadius, Id, Key, Response, Sense, Stroke, StrokeKind, TextEdit, Ui, Widget,
    WidgetInfo, vec2,
};
use egui_pop_model::values::{Descriptor, Type};

use crate::theme::Tokens;

/// A slider over an `f64`.
#[derive(Debug)]
pub struct Slider<'a> {
    value: &'a mut f64,
    label: String,
    soft: (f64, f64),
    hard: (f64, f64),
    step: f64,
    coarse: f64,
    default: Option<f64>,
    unit: String,
    precision: usize,
    scale: f64,
    integer: bool,
}

/// Parses a typed number: units after it are ignored, a comma is a
/// point.
pub fn parse_typed(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    let end = t
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || (*i == 0 && (*c == '-' || *c == '+'))))
        .map_or(t.len(), |(i, _)| i);
    t[..end].parse().ok()
}

impl<'a> Slider<'a> {
    /// A slider spanning `soft`, which is also its hard range until
    /// [`Slider::hard`] widens it.
    pub fn new(value: &'a mut f64, label: &str, soft: RangeInclusive<f64>) -> Self {
        let (lo, hi) = (*soft.start(), *soft.end());
        let step = if hi > lo { (hi - lo) / 100.0 } else { 0.01 };
        Self {
            value,
            label: label.into(),
            soft: (lo, hi),
            hard: (lo, hi),
            step,
            coarse: step * 10.0,
            default: None,
            unit: String::new(),
            precision: precision_of(step),
            scale: 1.0,
            integer: false,
        }
    }

    /// From a value descriptor: its label, ranges, steps, unit,
    /// precision, display scale and default.
    pub fn from_descriptor(value: &'a mut f64, d: &Descriptor) -> Self {
        let hard = d.hard.unwrap_or((0.0, 1.0));
        let soft = d.soft.unwrap_or(hard);
        let mut s = Self::new(value, &d.label, soft.0..=soft.1)
            .hard(hard.0..=hard.1)
            .step(d.step)
            .coarse(d.coarse)
            .precision(d.precision as usize);
        s.scale = d.display_scale;
        if let Some(u) = &d.unit {
            s.unit = u.clone();
        }
        if let Some(v) = d.default.as_f64() {
            s.default = Some(v);
        }
        s.integer = d.ty == Type::Int;
        s
    }

    /// The range a typed value may reach.
    pub fn hard(mut self, hard: RangeInclusive<f64>) -> Self {
        self.hard = (hard.start().min(self.soft.0), hard.end().max(self.soft.1));
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        if step > 0.0 {
            self.step = step;
            self.coarse = step * 10.0;
            self.precision = precision_of(step);
        }
        self
    }

    pub fn coarse(mut self, coarse: f64) -> Self {
        if coarse > 0.0 {
            self.coarse = coarse;
        }
        self
    }

    /// What a double click goes back to.
    pub fn default(mut self, default: f64) -> Self {
        self.default = Some(default);
        self
    }

    pub fn unit(mut self, unit: &str) -> Self {
        self.unit = unit.into();
        self
    }

    pub fn precision(mut self, decimals: usize) -> Self {
        self.precision = decimals;
        self
    }

    /// Whole numbers only.
    pub fn integer(mut self) -> Self {
        self.integer = true;
        self.precision = 0;
        self
    }

    fn shown(&self, v: f64) -> String {
        let n = format!("{:.*}", self.precision, v * self.scale);
        if self.unit.is_empty() {
            n
        } else {
            format!("{n} {}", self.unit)
        }
    }

    fn settle(&self, v: f64, (lo, hi): (f64, f64)) -> f64 {
        let v = if self.integer { v.round() } else { v };
        v.clamp(lo, hi)
    }
}

fn precision_of(step: f64) -> usize {
    if step >= 1.0 || step <= 0.0 {
        0
    } else {
        (-step.log10()).ceil() as usize
    }
}

impl Widget for Slider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = Tokens::of(ui.ctx());
        let height = ui.spacing().interact_size.y;
        let width = ui.available_width().max(120.0);
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
        let typing_id = response.id.with("egui_pop_slider_typing");
        let typing: Option<String> = ui.data(|d| d.get_temp(typing_id));
        let before = *self.value;
        let (lo, hi) = self.soft;

        if let Some(mut text) = typing {
            // Typed entry over the bar.
            let edit = ui.put(
                rect,
                TextEdit::singleline(&mut text)
                    .id(typing_id.with("edit"))
                    .horizontal_align(egui::Align::Center),
            );
            edit.request_focus();
            let done = ui.input(|i| i.key_pressed(Key::Enter));
            let cancel = ui.input(|i| i.key_pressed(Key::Escape)) || edit.lost_focus() && !done;
            if done {
                if let Some(v) = parse_typed(&text) {
                    *self.value = self.settle(v / self.scale, self.hard);
                }
                ui.data_mut(|d| d.remove::<String>(typing_id));
                response.request_focus();
            } else if cancel {
                ui.data_mut(|d| d.remove::<String>(typing_id));
            } else {
                ui.data_mut(|d| d.insert_temp(typing_id, text));
            }
        } else {
            // A drag accumulates exactly; only what is written is rounded.
            let acc_id = response.id.with("egui_pop_slider_acc");
            if response.drag_started() {
                ui.data_mut(|d| d.insert_temp(acc_id, *self.value));
            }
            if response.dragged() {
                let fine = if ui.input(|i| i.modifiers.shift) {
                    0.1
                } else {
                    1.0
                };
                let delta = f64::from(response.drag_delta().x / rect.width()) * (hi - lo) * fine;
                if delta != 0.0 {
                    let acc: f64 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(*self.value);
                    let next = (acc + delta).clamp(lo.min(acc), hi.max(acc));
                    ui.data_mut(|d| d.insert_temp(acc_id, next));
                    *self.value = self.settle(next, self.hard);
                }
            }
            if response.double_clicked()
                && let Some(d) = self.default
            {
                *self.value = d;
            }
            if response.clicked() {
                response.request_focus();
            }
            if response.has_focus() {
                let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
                let step = if shift {
                    self.coarse
                } else if ctrl {
                    self.step / 10.0
                } else {
                    self.step
                };
                let mut v = *self.value;
                ui.input(|i| {
                    if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp) {
                        v += step;
                    }
                    if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown) {
                        v -= step;
                    }
                    if i.key_pressed(Key::Home) {
                        v = lo;
                    }
                    if i.key_pressed(Key::End) {
                        v = hi;
                    }
                });
                let wheel: f32 = ui.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|e| match e {
                            egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                            _ => None,
                        })
                        .sum()
                });
                if response.hovered() && wheel != 0.0 {
                    v += step * f64::from(wheel.signum());
                }
                if v != *self.value {
                    // Keys stay in the soft range, unless already past it.
                    let (a, b) = (lo.min(*self.value), hi.max(*self.value));
                    *self.value = self.settle(v, (a, b));
                }
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    let text = format!("{:.*}", self.precision, *self.value * self.scale);
                    ui.data_mut(|d| d.insert_temp(typing_id, text));
                }
            }
            // The bar: track, fill to the value, label and value inside.
            let painter = ui.painter_at(rect);
            let radius = CornerRadius::same(t.radius.widget.round() as u8);
            let visuals = ui.style().interact(&response);
            painter.rect(
                rect,
                radius,
                t.slider.track,
                Stroke::NONE,
                StrokeKind::Inside,
            );
            let frac = if hi > lo {
                ((*self.value - lo) / (hi - lo)).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            // The fill is a strip along the bottom, so the label and the
            // value stay on the track's colour (one contrast pair).
            let strip = (rect.height() * 0.15).max(3.0);
            let fill = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - strip),
                egui::pos2(rect.left() + rect.width() * frac, rect.bottom()),
            );
            painter.rect(
                fill,
                radius,
                t.slider.fill,
                Stroke::NONE,
                StrokeKind::Inside,
            );
            let x = rect.left() + rect.width() * frac;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(2.0, t.slider.thumb),
            );
            if response.has_focus() {
                painter.rect_stroke(
                    rect,
                    radius,
                    Stroke::new(2.0, t.focus_ring),
                    StrokeKind::Inside,
                );
            } else if response.hovered() {
                painter.rect_stroke(rect, radius, visuals.bg_stroke, StrokeKind::Inside);
            }
            let pad = t.space.s;
            let font = egui::TextStyle::Button.resolve(ui.style());
            let past = *self.value < lo || *self.value > hi;
            painter.text(
                rect.left_center() + vec2(pad, 0.0),
                Align2::LEFT_CENTER,
                &self.label,
                font.clone(),
                t.text.default,
            );
            painter.text(
                rect.right_center() - vec2(pad, 0.0),
                Align2::RIGHT_CENTER,
                self.shown(*self.value),
                font,
                if past {
                    t.status.warning
                } else {
                    t.text.default
                },
            );
        }
        if *self.value != before {
            response.mark_changed();
        }
        let (label, value, enabled) = (self.label.clone(), *self.value, ui.is_enabled());
        response.widget_info(|| WidgetInfo::slider(enabled, value, &label));
        response
    }
}

/// The id of a slider's typed entry (for tests).
pub fn typing_id(slider: Id) -> Id {
    slider.with("egui_pop_slider_typing")
}

#[cfg(test)]
mod tests {
    use super::parse_typed;

    #[test]
    fn typed_values_keep_the_number_and_drop_the_unit() {
        assert_eq!(parse_typed("1,5 EV"), Some(1.5));
        assert_eq!(parse_typed(" -0.25"), Some(-0.25));
        assert_eq!(parse_typed("+3%"), Some(3.0));
        assert_eq!(parse_typed("EV"), None);
    }
}
