//! A parameter widget: the editor a value descriptor calls for, in any
//! of the three binding modes (ADR 0009). A float or an int is a
//! [`Slider`], a bool a check box, a choice a
//! drop-down, three or four floats in `[0, 1]` a colour swatch, a text a
//! field; every editor is named by the descriptor's label.
//!
//! ```
//! use egui_pop_core::param::{self, Mode};
//! use egui_pop_model::values::{Descriptor, Value};
//!
//! fn gain(ui: &mut egui::Ui, store: &egui_pop_core::binding::SharedStore, local: &mut Value) {
//!     let d = Descriptor::float("Gain", 0.0, 2.0, 1.0);
//!     param::param(ui, &d, Mode::Plain(local));
//!     param::param(ui, &d, Mode::Bound(store, "params/gain"));
//!     param::param(ui, &d, Mode::Watch(store, "params/gain"));
//! }
//! ```

use egui::{Response, Ui};
use egui_pop_model::values::{Descriptor, Type, Value};

use crate::binding::{self, Bindable, Binding, Choice, SharedStore};
use crate::slider::Slider;
use crate::ui::UiPop as _;
use crate::widgets;

/// Where the value is.
#[derive(Debug)]
pub enum Mode<'a> {
    /// The caller's value.
    Plain(&'a mut Value),
    /// A store's path, read and written as the user.
    Bound(&'a SharedStore, &'a str),
    /// A store's path, shown only.
    Watch(&'a SharedStore, &'a str),
}

fn with<T: Bindable>(
    ui: &mut Ui,
    mode: Mode<'_>,
    widget: impl FnOnce(&mut Ui, &mut T) -> Response,
) -> Response {
    match mode {
        Mode::Plain(v) => {
            let mut t = T::from_value(v).unwrap_or_default();
            let before = t.clone();
            let r = widget(ui, &mut t);
            if t != before {
                *v = t.to_value();
            }
            r
        }
        Mode::Bound(store, path) => binding::edit(ui, Binding::bound(store, path), widget),
        Mode::Watch(store, path) => binding::edit(ui, Binding::watch(store, path), widget),
    }
}

/// A label, then `add`'s widget named by it.
fn labelled(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> Response) -> Response {
    ui.horizontal(|ui| {
        let l = ui.label(label);
        add(ui).labelled_by(l.id)
    })
    .inner
}

/// Whether a descriptor is a colour: three or four floats in `[0, 1]`.
pub fn is_colour(d: &Descriptor) -> bool {
    d.ty == Type::Floats
        && matches!(d.count, Some(3 | 4))
        && d.hard.is_some_and(|(lo, hi)| lo >= 0.0 && hi <= 1.0)
}

/// Draws the editor `d` calls for.
pub fn param(ui: &mut Ui, d: &Descriptor, mode: Mode<'_>) -> Response {
    param_shown(ui, d, mode, &|c| c.to_string())
}

/// As [`param`], a choice's values shown through `shown` (a translation:
/// `strings::setting_choice`); the value kept is the raw one.
pub fn param_shown(
    ui: &mut Ui,
    d: &Descriptor,
    mode: Mode<'_>,
    shown: &dyn Fn(&str) -> String,
) -> Response {
    match d.ty {
        Type::Float => with(ui, mode, |ui, v: &mut f64| {
            ui.add(Slider::from_descriptor(v, d))
        }),
        Type::Int => with(ui, mode, |ui, v: &mut i64| {
            let mut f = *v as f64;
            let r = ui.add(Slider::from_descriptor(&mut f, d).integer());
            *v = f.round() as i64;
            r
        }),
        Type::Bool => with(ui, mode, |ui, v: &mut bool| {
            widgets::checkbox(ui, v, &d.label)
        }),
        Type::Choice => labelled(ui, &d.label, |ui| {
            with(ui, mode, |ui, v: &mut Choice| {
                let mut picked = v.0.clone();
                let combo =
                    widgets::combo_box(("egui_pop.param", &d.label)).selected_text(shown(&picked));
                let r = ui
                    .combo_box(combo, |ui| {
                        for c in &d.choices {
                            widgets::selectable_value(ui, &mut picked, c.clone(), shown(c));
                        }
                    })
                    .response;
                if picked != v.0 {
                    v.0 = picked;
                }
                r
            })
        }),
        Type::Floats if is_colour(d) => labelled(ui, &d.label, |ui| {
            with(ui, mode, |ui, v: &mut Vec<f64>| {
                v.resize(d.count.unwrap_or(3) as usize, 1.0);
                let before = [v[0] as f32, v[1] as f32, v[2] as f32];
                let mut rgb = before;
                let r = ui.color_edit_button_rgb(&mut rgb);
                // Only an edit goes back: egui's colour round trip moves
                // the last bits of an untouched value.
                if r.changed() && rgb != before {
                    for (i, c) in rgb.iter().enumerate() {
                        v[i] = f64::from(*c);
                    }
                }
                r
            })
        }),
        Type::Text => labelled(ui, &d.label, |ui| {
            with(ui, mode, |ui, v: &mut String| {
                let r = ui.text_edit_singleline(v);
                crate::focus_mode::follow_pointer(&r);
                r
            })
        }),
        Type::Floats | Type::Ints => labelled(ui, &d.label, |ui| {
            let shown = match mode {
                Mode::Plain(v) => v.to_json().to_string(),
                Mode::Bound(s, p) | Mode::Watch(s, p) => crate::lock(s)
                    .get(p)
                    .map(|e| e.value.to_json().to_string())
                    .unwrap_or_default(),
            };
            ui.monospace(shown)
        }),
    }
}

/// The value a mode holds now (for display beside the editor).
pub fn current(mode: &Mode<'_>) -> Option<Value> {
    match mode {
        Mode::Plain(v) => Some((**v).clone()),
        Mode::Bound(s, p) | Mode::Watch(s, p) => crate::lock(s).get(p).map(|e| e.value.clone()),
    }
}
