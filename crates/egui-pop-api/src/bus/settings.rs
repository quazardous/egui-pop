//! Settings changed, on the bus (ADR 0029).
//!
//! The value store is the one writer of a setting's value (ADR 0016):
//! every change of a `settings/` path, made here (the Settings app, a
//! bound widget, `value.set`, the application's code) or by another
//! client of the host's store (applied by the Settings app's sync), is
//! said **once** as `pop.settings.changed` ([`SettingsChanged`], a
//! [`SettingChange`]: path, scope, by). Whoever follows a setting is one
//! of its listeners and does its own part there; nobody compares a
//! setting every pass.
//!
//! The store tells its listeners under its lock, where nothing may read
//! it back, so the change is said right after the write, at the next of
//! these points, all on the interface's thread: the start of the
//! application's logic, after the requests it ran, after the Settings
//! app's sync applied changes made elsewhere, and at the end of the main
//! window's pass ([`AppHandle::deliver_settings`]). A change heard from
//! another client is therefore followed in the pass it is heard.
//!
//! egui-pop's own listeners, local (they touch the interface's state):
//! [`THEME_INSTALL`] (the accent, the text size and fonts, the scroll
//! bars, the light or dark preference), [`POINTER_PROFILE`] (the
//! pointer profile's bindings), [`FRAME_DECORATIONS`] (the main window's
//! decorations for `window.frame`).

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use bus_contract::{Point, PointKind};
use egui_pop_core::lock;
use egui_pop_model::values::Store;
use serde::Serialize;

use super::emit_with;
use crate::app::AppHandle;

/// A setting changed: its store path (`settings/<id>`), the value scope
/// it changed in (`app`, a host's scope such as `library`, `run`), who
/// wrote it (`user`, `agent:<session>`, `store` for another client of
/// the host's store) and the value store's sequence number of the write.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct SettingChange {
    pub path: String,
    pub scope: String,
    pub by: String,
    pub seq: u64,
}

impl SettingChange {
    pub fn new(path: impl Into<String>, scope: impl Into<String>, by: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            scope: scope.into(),
            by: by.into(),
            seq: 0,
        }
    }

    /// The setting's id: the path without `settings/`.
    pub fn id(&self) -> &str {
        self.path.strip_prefix(PREFIX).unwrap_or(&self.path)
    }

    /// Whether it is one of `ids`.
    pub fn is_any(&self, ids: &[egui_pop_model::ids::SettingId]) -> bool {
        ids.iter().any(|id| id.as_str() == self.id())
    }
}

/// A setting changed (`pop.settings.changed`), said once per change of its
/// value by the value store, its one writer.
#[derive(Debug)]
pub struct SettingsChanged;
impl Point for SettingsChanged {
    const KIND: PointKind = PointKind::Event;
    const ID: &'static str = "pop.settings.changed";
    type Input = SettingChange;
    type Value = ();
}

/// The settings points, in the order [`super::order_dump`] prints them.
pub const SETTINGS_POINTS: [&str; 1] = [SettingsChanged::ID];

/// egui-pop's listener installing the theme a setting changes.
pub const THEME_INSTALL: &str = "pop.theme.install";
/// egui-pop's listener loading the pointer profile the setting names.
pub const POINTER_PROFILE: &str = "pop.pointer.profile";
/// egui-pop's listener giving the main window the decorations
/// `window.frame` asks for.
pub const FRAME_DECORATIONS: &str = "pop.frame.decorations";

/// Where settings live in the value store.
const PREFIX: &str = "settings/";

/// How many rounds a delivery runs: a listener writing a setting is
/// followed in the same delivery, a loop is cut.
const ROUNDS: usize = 16;

thread_local! {
    /// The scope and the writer the Settings app names while it writes
    /// what a layer or another client changed.
    static WRITING: RefCell<Vec<(String, Option<String>)>> = const { RefCell::new(Vec::new()) };
}

/// Names the scope (and the writer, when it is not the write's origin)
/// of the writes made on this thread until dropped.
pub(crate) fn writing(scope: &str, by: Option<&str>) -> Writing {
    WRITING.with_borrow_mut(|w| w.push((scope.to_string(), by.map(str::to_string))));
    Writing(std::marker::PhantomData)
}

/// Ends [`writing`].
#[must_use]
pub(crate) struct Writing(std::marker::PhantomData<*const ()>);

impl Drop for Writing {
    fn drop(&mut self) {
        WRITING.with_borrow_mut(|w| w.pop());
    }
}

/// The changes the value store told and not said yet.
pub(crate) type Heard = Arc<Mutex<Vec<SettingChange>>>;

/// Listens to the value store's `settings/` paths: each change is kept
/// for [`AppHandle::deliver_settings`].
pub(crate) fn hear(store: &mut Store, heard: &Heard) {
    let heard = heard.clone();
    store.listen(&egui_pop_model::ids::OwnerId::FRAMEWORK, PREFIX, move |c| {
        let (scope, by) = WRITING
            .with_borrow(|w| w.last().cloned())
            .unwrap_or_else(|| ("app".to_string(), None));
        lock(&heard).push(SettingChange {
            path: c.path.clone(),
            scope,
            by: by.unwrap_or_else(|| c.origin.to_string()),
            seq: c.seq,
        });
    });
}

impl AppHandle {
    /// Says each setting changed since the last time as
    /// `pop.settings.changed`, once, in the order the value store wrote them:
    /// its listeners run now, on this thread. A listener writing a
    /// setting is followed in the same call. Asked from inside a
    /// delivery, it returns at once: the delivery running takes it.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn deliver_settings(&self) {
        let bus = &self.0.bus;
        if bus.settings_delivering.replace(true) {
            return;
        }
        let _delivering = super::Delivering(&bus.settings_delivering);
        for _ in 0..ROUNDS {
            let changes = std::mem::take(&mut *lock(&bus.settings_heard));
            if changes.is_empty() {
                return;
            }
            for change in changes {
                emit_with::<SettingsChanged>(self, change, ());
            }
        }
        log::warn!(target: "egui_pop", "settings.changed: listeners kept changing settings; the rest waits for the next pass");
    }
}
