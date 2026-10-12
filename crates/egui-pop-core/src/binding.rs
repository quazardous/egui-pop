//! Three ways to bind a widget's value (ADR 0009): plain `&mut T` as in
//! egui, bound to a value store (writes with an origin, a drag is one
//! gesture, listeners told), or watch-only.
//!
//! ```
//! use std::sync::{Arc, Mutex};
//! use egui_pop_core::binding::{self, Binding};
//! use egui_pop_model::ids::OwnerId;
//! use egui_pop_model::values::{Descriptor, Store};
//!
//! let store = Arc::new(Mutex::new(Store::new()));
//! store.lock().unwrap()
//!     .declare(&OwnerId::FRAMEWORK, "settings/volume", Descriptor::float("Volume", 0.0, 1.0, 0.5))
//!     .unwrap();
//!
//! fn draw(ui: &mut egui::Ui, store: &binding::SharedStore, local: &mut f64) {
//!     // As in egui:
//!     binding::edit(ui, Binding::plain(local), |ui, v| ui.add(egui::Slider::new(v, 0.0..=1.0)));
//!     // Bound to the store:
//!     binding::edit(ui, Binding::bound(store, "settings/volume"), |ui, v: &mut f64| {
//!         ui.add(egui::Slider::new(v, 0.0..=1.0))
//!     });
//!     // Shown, never written:
//!     binding::edit(ui, Binding::watch(store, "settings/volume"), |ui, v: &mut f64| {
//!         ui.add(egui::Slider::new(v, 0.0..=1.0))
//!     });
//! }
//! ```

use std::sync::{Arc, Mutex};

use egui::{Id, Response, Ui};
use egui_pop_model::commands::Origin;
use egui_pop_model::values::{Change, GestureId, Set, Store, Value};

use crate::state::lock;
use crate::strings::{self, HELD_BY, NO_VALUE, READ_ONLY};

/// A store shared between the application, its widgets and its commands.
pub type SharedStore = Arc<Mutex<Store>>;

/// A type a widget edits that the store can hold.
pub trait Bindable: Clone + PartialEq + Default {
    fn from_value(value: &Value) -> Option<Self>;
    fn to_value(&self) -> Value;
}

impl Bindable for f64 {
    fn from_value(v: &Value) -> Option<Self> {
        v.as_f64()
    }
    fn to_value(&self) -> Value {
        Value::Float(*self)
    }
}

impl Bindable for f32 {
    fn from_value(v: &Value) -> Option<Self> {
        v.as_f64().map(|v| v as f32)
    }
    fn to_value(&self) -> Value {
        Value::Float(*self as f64)
    }
}

impl Bindable for i64 {
    fn from_value(v: &Value) -> Option<Self> {
        v.as_i64().or_else(|| v.as_f64().map(|f| f.round() as i64))
    }
    fn to_value(&self) -> Value {
        Value::Int(*self)
    }
}

impl Bindable for i32 {
    fn from_value(v: &Value) -> Option<Self> {
        i64::from_value(v).map(|v| v as i32)
    }
    fn to_value(&self) -> Value {
        Value::Int(*self as i64)
    }
}

impl Bindable for bool {
    fn from_value(v: &Value) -> Option<Self> {
        v.as_bool()
    }
    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
}

impl Bindable for String {
    fn from_value(v: &Value) -> Option<Self> {
        v.as_text().map(str::to_string)
    }
    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }
}

/// How a widget's value is held.
#[derive(Debug)]
pub enum Binding<'a, T> {
    /// The caller's value, as in egui.
    Plain(&'a mut T),
    /// A path of a store, read and written.
    Bound {
        store: &'a SharedStore,
        path: &'a str,
    },
    /// A path of a store, read only.
    Watch {
        store: &'a SharedStore,
        path: &'a str,
    },
}

impl<'a, T> Binding<'a, T> {
    pub fn plain(value: &'a mut T) -> Self {
        Binding::Plain(value)
    }

    pub fn bound(store: &'a SharedStore, path: &'a str) -> Self {
        Binding::Bound { store, path }
    }

    pub fn watch(store: &'a SharedStore, path: &'a str) -> Self {
        Binding::Watch { store, path }
    }
}

fn gesture_id(response: &Response) -> Id {
    response.id.with("egui_pop_gesture")
}

fn cancel_id(response_id: Id) -> Id {
    response_id.with("egui_pop_cancel")
}

/// Tells the binding around a widget that its gesture is given up (the
/// widget has put its value back): bound to a store, the gesture's
/// writes are undone and leave no undo step.
pub fn cancel_gesture(ctx: &egui::Context, response_id: Id) {
    ctx.data_mut(|d| d.insert_temp(cancel_id(response_id), true));
}

fn error_id(response_id: Id) -> Id {
    response_id.with("egui_pop_refused")
}

/// Draws a widget over a binding. `widget` edits a local copy; what
/// changed goes back as the binding says.
pub fn edit<T: Bindable>(
    ui: &mut Ui,
    binding: Binding<'_, T>,
    widget: impl FnOnce(&mut Ui, &mut T) -> Response,
) -> Response {
    match binding {
        Binding::Plain(v) => widget(ui, v),
        Binding::Watch { store, path } => {
            let mut v = read(store, path).unwrap_or_default();
            let r = ui.add_enabled_ui(false, |ui| widget(ui, &mut v)).inner;
            r.on_disabled_hover_text(strings::tr(ui.ctx(), &READ_ONLY))
        }
        Binding::Bound { store, path } => {
            let (current, held) = {
                let s = lock(store);
                match s.get(path) {
                    Some(e) => (
                        T::from_value(&e.value),
                        s.held_by(path).filter(|o| **o != Origin::User).cloned(),
                    ),
                    None => (None, None),
                }
            };
            let Some(mut v) = current else {
                let mut dummy = T::default();
                let r = ui.add_enabled_ui(false, |ui| widget(ui, &mut dummy)).inner;
                log::warn!(target: "egui_pop", "binding to `{path}`: no such value (or not this type); the widget is disabled");
                return r.on_disabled_hover_text(strings::tr_args(
                    ui.ctx(),
                    &NO_VALUE,
                    &[("path", path)],
                ));
            };
            let before = v.clone();
            let mut r = widget(ui, &mut v);
            let gid = gesture_id(&r);
            if r.drag_started() {
                let g = lock(store).begin_gesture(Origin::User, &format!("drag {path}"));
                ui.ctx().data_mut(|d| d.insert_temp(gid, g.0));
            }
            let gesture = ui.ctx().data(|d| d.get_temp::<u64>(gid)).map(GestureId);
            let cancelled = ui
                .ctx()
                .data_mut(|d| d.remove_temp::<bool>(cancel_id(r.id)))
                .unwrap_or(false);
            if cancelled {
                // The widget gave the gesture up (Escape during a drag):
                // the store puts the values back, no undo step is left.
                if let Some(g) = gesture {
                    lock(store).cancel(g);
                    ui.ctx().data_mut(|d| d.remove::<u64>(gid));
                }
                return r;
            }
            if v != before {
                let mut set = Set::new(path, Change::To(v.to_value()), Origin::User);
                set.gesture = gesture;
                let outcome = lock(store).set(set);
                match outcome {
                    Ok(_) => ui.ctx().data_mut(|d| d.remove::<String>(error_id(r.id))),
                    Err(refused) => {
                        log::warn!(target: "egui_pop", "write to `{path}` refused: {refused}");
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(error_id(r.id), refused.to_string()));
                    }
                }
            }
            if r.drag_stopped()
                && let Some(g) = gesture
            {
                lock(store).end(g);
                ui.ctx().data_mut(|d| d.remove::<u64>(gid));
            }
            if let Some(by) = held {
                r = r.on_hover_text(strings::tr_args(
                    ui.ctx(),
                    &HELD_BY,
                    &[("origin", &format!("{by:?}"))],
                ));
            }
            if let Some(e) = ui.ctx().data(|d| d.get_temp::<String>(error_id(r.id))) {
                r = r.on_hover_text(e);
            }
            r
        }
    }
}

/// Reads a path as a type.
pub fn read<T: Bindable>(store: &SharedStore, path: &str) -> Option<T> {
    lock(store).get(path).and_then(|e| T::from_value(&e.value))
}

/// One of a descriptor's choices, as a widget edits it (a `String` would
/// be written as free text).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice(pub String);

impl Bindable for Choice {
    fn from_value(v: &Value) -> Option<Self> {
        v.as_choice().map(|c| Choice(c.to_string()))
    }
    fn to_value(&self) -> Value {
        Value::Choice(self.0.clone())
    }
}

impl Bindable for Vec<f64> {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Floats(f) => Some(f.clone()),
            _ => None,
        }
    }
    fn to_value(&self) -> Value {
        Value::Floats(self.clone())
    }
}
