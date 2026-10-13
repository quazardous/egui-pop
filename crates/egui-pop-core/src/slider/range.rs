//! A range slider: a low and a high value on one bar (an ISO, date or
//! focal length filter), with the gestures of [`Slider`](super::Slider).
//!
//! Each thumb is its own widget: it takes the focus (Tab goes from the
//! low to the high one), steps with the keys and the wheel, opens the
//! typed entry, and is an AccessKit slider named after the bar's label
//! ("ISO, minimum", "ISO, maximum"). A press on the bar takes the
//! nearer thumb: the half of the bar left of the point between the
//! thumbs drives the low one, the rest the high one. The thumbs never
//! cross. A double click resets both to the default.

use std::ops::RangeInclusive;

use egui::{Rect, Response, Sense, Ui, Widget, WidgetInfo, pos2, vec2};

use super::{
    Cause, Change, Drag, Hook, SliderOutput, Spec, Thumb, Verdict, access, decide, drag, gestures,
    keep_auto_ids, keys_wheel_access, modifier_scale, open_typed_entry, paint_bar, spec_builders,
    typed_entry, typing_id,
};
use crate::strings::{self, SLIDER_MAXIMUM, SLIDER_MINIMUM};

/// Two values on one bar, `low <= high`.
#[derive(Debug)]
pub struct RangeSlider<'a> {
    low: &'a mut f64,
    high: &'a mut f64,
    spec: Spec,
    default: Option<(f64, f64)>,
    hook: Option<Hook<'a>>,
}

impl<'a> RangeSlider<'a> {
    /// A range slider spanning `soft`, which is also its hard range until
    /// [`RangeSlider::hard`] widens it.
    pub fn new(
        low: &'a mut f64,
        high: &'a mut f64,
        label: &str,
        soft: RangeInclusive<f64>,
    ) -> Self {
        Self {
            low,
            high,
            spec: Spec::new(label, soft),
            default: None,
            hook: None,
        }
    }

    spec_builders!();

    /// What a double click goes back to.
    pub fn default(mut self, low: f64, high: f64) -> Self {
        self.default = Some((low.min(high), low.max(high)));
        self
    }

    /// Sees every change of either thumb before it is written
    /// ([`Change::thumb`] says which; see the slider module's "Hooks").
    pub fn on_change(mut self, hook: impl FnMut(&Change) -> Verdict + 'a) -> Self {
        self.hook = Some(Hook(Box::new(hook)));
        self
    }

    /// Draws the range slider; says what changed.
    pub fn show(self, ui: &mut Ui) -> SliderOutput {
        let RangeSlider {
            low,
            high,
            spec,
            default,
            mut hook,
        } = self;
        if *low > *high {
            std::mem::swap(low, high);
        }
        let height = ui.spacing().interact_size.y;
        let width = ui.available_width().max(120.0);
        let (id, bar) = ui.allocate_space(vec2(width, height));
        let (low_rect, high_rect) = halves(bar, &spec, *low, *high);
        let mut lo_r = ui.interact(low_rect, id.with("low"), Sense::click_and_drag());
        let mut hi_r = ui.interact(high_rect, id.with("high"), Sense::click_and_drag());
        let (before_lo, before_hi) = (*low, *high);
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
        let mut typing = false;
        for thumb in [Thumb::Low, Thumb::High] {
            let (response, value, bounds) = match thumb {
                Thumb::Low => (&mut lo_r, &mut *low, (spec.soft.0, *high)),
                _ => (&mut hi_r, &mut *high, (*low, spec.soft.1)),
            };
            let change = |from: f64, to: f64, cause: Cause, done: bool| Change {
                thumb,
                from,
                to,
                cause,
                done,
            };
            // A typed value stays on its side of the other thumb.
            let hard = match thumb {
                Thumb::Low => (spec.hard.0, bounds.1),
                _ => (bounds.0, spec.hard.1),
            };
            let tid = typing_id(response.id);
            if let Some(text) = ui.data(|d| d.get_temp::<String>(tid)) {
                typing = true;
                if let Some(end) = typed_entry(ui, bar, tid, text, &spec, *value) {
                    if let Some(v) = end.value {
                        let to = spec.settle(v, hard);
                        if to != *value {
                            apply(change(*value, to, Cause::Typed, true), value);
                        }
                    }
                    if end.refocus {
                        response.request_focus();
                    }
                }
                continue;
            }
            keep_auto_ids(ui);
            let g = gestures(ui.ctx());
            let scale = modifier_scale(ui, &g);
            match drag(ui, response, &spec, *value, bounds, scale, bar.width()) {
                Drag::None => {}
                Drag::Moved(v) => {
                    let to = spec.settle(v, hard);
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
            if response.clicked() {
                // A click focuses; it never makes the value jump.
                response.request_focus();
            }
            if let Some((v, cause)) =
                keys_wheel_access(ui, response, &spec, *value, bounds, scale, &g)
            {
                let to = spec.settle(v, hard);
                if to != *value {
                    apply(change(*value, to, cause, true), value);
                }
            }
            if response.has_focus() && ui.input(|i| i.key_pressed(g.typed_entry)) {
                open_typed_entry(ui, tid, spec.number(ui.ctx(), *value));
            }
        }
        let g = gestures(ui.ctx());
        if !typing
            && g.reset_on_double_click
            && (lo_r.double_clicked() || hi_r.double_clicked())
            && let Some((dl, dh)) = default
        {
            for (thumb, value, to) in [(Thumb::Low, &mut *low, dl), (Thumb::High, &mut *high, dh)] {
                if to != *value {
                    let c = Change {
                        thumb,
                        from: *value,
                        to,
                        cause: Cause::Reset,
                        done: true,
                    };
                    apply(c, value);
                }
            }
        }
        // A hook may redirect a thumb past the other one: keep the order.
        if *low > *high {
            std::mem::swap(low, high);
        }
        if !typing {
            let text = spec.with_unit(format!(
                "{} – {}",
                spec.number(ui.ctx(), *low),
                spec.number(ui.ctx(), *high)
            ));
            let past = *low < spec.soft.0 || *high > spec.soft.1;
            paint_bar(ui, bar, &[&lo_r, &hi_r], &spec, &[*low, *high], &text, past);
        }
        let enabled = ui.is_enabled();
        for (r, v, before, key) in [
            (&mut lo_r, *low, before_lo, SLIDER_MINIMUM),
            (&mut hi_r, *high, before_hi, SLIDER_MAXIMUM),
        ] {
            if v != before {
                r.mark_changed();
            }
            let name = strings::tr_args(ui.ctx(), &key, &[("label", spec.label.as_str())]);
            r.widget_info(|| WidgetInfo::slider(enabled, v, &name));
        }
        access(ui, lo_r.id, &spec, *low, (spec.soft.0, *high));
        access(ui, hi_r.id, &spec, *high, (*low, spec.soft.1));
        SliderOutput {
            response: lo_r | hi_r,
            changes,
            quad: None,
        }
    }
}

impl Widget for RangeSlider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

/// The two halves of the bar each thumb answers in: split at the point
/// between the thumbs, kept a thumb's reach from either end.
fn halves(bar: Rect, spec: &Spec, low: f64, high: f64) -> (Rect, Rect) {
    let x = |v: f64| bar.left() + bar.width() * spec.frac(v).clamp(0.0, 1.0) as f32;
    let reach = (bar.width() / 4.0).min(bar.height());
    let mid = ((x(low) + x(high)) / 2.0).clamp(bar.left() + reach, bar.right() - reach);
    (
        Rect::from_min_max(bar.min, pos2(mid, bar.bottom())),
        Rect::from_min_max(pos2(mid, bar.top()), bar.max),
    )
}
