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
use egui_pop_model::ids::OwnerId;
use egui_pop_model::values::{Change, GestureId, Set, Store, Value};

use crate::state::{Slot, lock, shared};
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

/// A bound widget's gesture in the store, per widget.
const GESTURE: Slot<u64> = Slot::new("core.binding.gesture");
/// The widget gave its gesture up, per widget.
const CANCELLED: Slot<bool> = Slot::new("core.binding.cancelled");
/// Why the store refused the widget's last write, per widget.
const REFUSED: Slot<String> = Slot::new("core.binding.refused");

/// Tells the binding around a widget that its gesture is given up (the
/// widget has put its value back): bound to a store, the gesture's
/// writes are undone and leave no undo step.
pub fn cancel_gesture(ctx: &egui::Context, response_id: Id) {
    CANCELLED.keyed(response_id).set(ctx, true);
}

/// A bound widget's gesture still open: its window, its widget, its
/// store and the gesture in it.
struct OpenGesture {
    window: egui::ViewportId,
    widget: Id,
    store: SharedStore,
    gesture: GestureId,
}

impl std::fmt::Debug for OpenGesture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGesture")
            .field("window", &self.window)
            .field("widget", &self.widget)
            .field("gesture", &self.gesture)
            .finish_non_exhaustive()
    }
}

/// The bound widgets' gestures still open, and whether the end of pass
/// that cancels the orphans is registered.
#[derive(Debug, Default)]
struct Open {
    gestures: Vec<OpenGesture>,
    registered: bool,
}

fn open_gestures(ctx: &egui::Context) -> Arc<Mutex<Open>> {
    shared::<Open>(ctx, "core.binding.open")
}

/// Notes a gesture opened by a bound widget, so that it is cancelled if
/// the widget goes away with it open.
fn opened(ctx: &egui::Context, widget: Id, store: &SharedStore, gesture: GestureId) {
    let st = open_gestures(ctx);
    let mut open = lock(&st);
    open.gestures.retain(|g| g.widget != widget);
    open.gestures.push(OpenGesture {
        window: ctx.viewport_id(),
        widget,
        store: Arc::clone(store),
        gesture,
    });
    if !open.registered {
        open.registered = true;
        ctx.on_end_pass(
            "egui_pop_core::binding",
            Arc::new(|ui: &mut Ui| cancel_orphans(ui.ctx())),
        );
    }
}

/// The widget's gesture is over (ended or cancelled).
fn closed(ctx: &egui::Context, widget: Id) {
    lock(&open_gestures(ctx))
        .gestures
        .retain(|g| g.widget != widget);
}

/// At the end of a window's pass: a gesture whose widget was not drawn
/// in it (its host closed, by its close button or a command, with the
/// gesture open) is cancelled: the store puts its values back and no
/// undo step is left, and egui's drag of the gone widget is stopped.
fn cancel_orphans(ctx: &egui::Context) {
    let window = ctx.viewport_id();
    let orphans: Vec<OpenGesture> = {
        let st = open_gestures(ctx);
        let mut open = lock(&st);
        let (gone, kept) = std::mem::take(&mut open.gestures)
            .into_iter()
            .partition(|g| {
                g.window == window && !ctx.viewport(|v| v.this_pass.widgets.contains(g.widget))
            });
        open.gestures = kept;
        gone
    };
    for g in orphans {
        lock(&g.store).cancel(g.gesture);
        GESTURE.keyed(g.widget).remove(ctx);
        if ctx.dragged_id() == Some(g.widget) {
            ctx.stop_dragging();
        }
        log::debug!(target: "egui_pop", "binding: the widget {:?} went away mid-gesture; gesture cancelled", g.widget);
    }
}

/// Draws a widget over a binding. `widget` edits a local copy; what
/// changed goes back as the binding says.
///
/// Bound to a store, a drag is one gesture. It is cancelled (the store
/// puts the values back, no undo step is left) when the widget gives it
/// up ([`cancel_gesture`]), when Escape ends the drag (see
/// [`focus`](crate::focus#escape-in-order)), and when the widget is not
/// drawn in a pass while it is open (its host closed mid-drag).
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
            let gid = GESTURE.keyed(r.id);
            if r.drag_started() {
                let g = lock(store).begin_gesture(Origin::User, &format!("drag {path}"));
                gid.set(ui.ctx(), g.0);
                opened(ui.ctx(), r.id, store, g);
            }
            let gesture = gid.get(ui.ctx()).map(GestureId);
            let cancelled = CANCELLED.keyed(r.id).remove(ui.ctx()).unwrap_or(false)
                || (r.drag_stopped() && crate::focus::escape_cancelled(ui.ctx()).is_some());
            if cancelled {
                // The widget gave the gesture up, or Escape ended the drag
                // (an egui widget does not put its value back itself):
                // the store puts the values back, no undo step is left.
                if let Some(g) = gesture {
                    lock(store).cancel(g);
                    gid.remove(ui.ctx());
                    closed(ui.ctx(), r.id);
                }
                return r;
            }
            if v != before {
                // A person's edit: the framework writes it, whoever drew
                // the widget.
                let mut set =
                    Set::new(path, Change::To(v.to_value()), Origin::User).by(&OwnerId::FRAMEWORK);
                set.gesture = gesture;
                let outcome = lock(store).set(set);
                match outcome {
                    Ok(_) => {
                        REFUSED.keyed(r.id).remove(ui.ctx());
                    }
                    Err(refused) => {
                        log::warn!(target: "egui_pop", "write to `{path}` refused: {refused}");
                        REFUSED.keyed(r.id).set(ui.ctx(), refused.to_string());
                    }
                }
            }
            if r.drag_stopped()
                && let Some(g) = gesture
            {
                lock(store).end(g);
                gid.remove(ui.ctx());
                closed(ui.ctx(), r.id);
            }
            if let Some(by) = held {
                r = r.on_hover_text(strings::tr_args(
                    ui.ctx(),
                    &HELD_BY,
                    &[("origin", &format!("{by:?}"))],
                ));
            }
            if let Some(e) = REFUSED.keyed(r.id).get(ui.ctx()) {
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
