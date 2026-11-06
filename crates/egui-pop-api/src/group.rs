//! One undo step for many writes: [`AppHandle::group`] (and
//! [`Call::group`] in a command) opens a history group through the host
//! ([`Host::begin_group`](crate::host::Host::begin_group)), runs a
//! closure, and ends the group when the closure succeeds or cancels it
//! (every write put back) when it fails or panics.
//!
//! The group goes through the host, so a host keeping its own history
//! sees it. While the host dispatches a command it is out of reach; a
//! command's group then goes through the host's
//! [`Host::groups`](crate::host::Host::groups), or the store's groups
//! when the host gives none.
//!
//! ```
//! use egui_pop_api::{App, Ctx, Plugin, PluginError};
//! use egui_pop_model::commands::{CommandSpec, Origin};
//! use egui_pop_model::ids::{CommandId, PluginId};
//! use egui_pop_model::values::{Change, Descriptor, Value};
//!
//! const RESET: CommandId = CommandId::from_static("swatch.reset");
//!
//! struct Swatch;
//! impl Plugin for Swatch {
//!     fn id(&self) -> PluginId { PluginId::from_static("swatch") }
//!     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
//!         ctx.value("swatch/hue", Descriptor::float("Hue", 0.0, 360.0, 200.0))?;
//!         ctx.value("swatch/size", Descriptor::float("Size", 40.0, 300.0, 160.0))?;
//!         ctx.command(CommandSpec::new(RESET, "Reset", "Swatch"), |call| {
//!             call.group("Reset the swatch", |g| {
//!                 g.set("swatch/hue", Change::Reset)?;
//!                 g.set("swatch/size", Change::Reset)?;
//!                 Ok(serde_json::Value::Null)
//!             })
//!         })?;
//!         Ok(())
//!     }
//! }
//!
//! let mut app = App::builder().plugin(Swatch).build(&egui::Context::default()).unwrap();
//! let h = app.handle().clone();
//! h.group(&Origin::User, "Move", |g| {
//!     g.set("swatch/hue", Change::To(Value::Float(30.0)))?;
//!     g.set("swatch/size", Change::To(Value::Float(80.0)))
//! }).unwrap();
//! app.run(&RESET, serde_json::Value::Null, Origin::User);
//! assert_eq!(h.value::<f64>("swatch/hue"), Some(200.0));
//! let store = h.store();
//! let steps = egui_pop_core::lock(&store).undo_steps().to_vec();
//! assert_eq!(steps.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["Move", "Reset the swatch"]);
//! ```

use std::fmt;
use std::rc::Rc;

use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::values::{Applied, Change, GestureId, Refused, Set};

use crate::app::{AppHandle, Call};
use crate::host::Groups;

/// What opened a group, so the same one closes it.
enum Route {
    Host,
    Groups(Rc<dyn Groups>),
    Store,
}

/// A history group being written: what is written through it until the
/// closure returns is one undo step, labelled and with its origin.
pub struct Group<'a> {
    app: &'a AppHandle,
    origin: Origin,
    gesture: Option<GestureId>,
}

impl fmt::Debug for Group<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Group")
            .field("origin", &self.origin)
            .field("gesture", &self.gesture)
            .finish_non_exhaustive()
    }
}

impl Group<'_> {
    /// The group's gesture, to carry on writes made elsewhere
    /// (`Set::gesture`); `None` when the host groups nothing. With a host
    /// keeping its own history, it is that history's: writes to the
    /// host's own values carry it.
    pub fn gesture(&self) -> Option<GestureId> {
        self.gesture
    }

    /// Who the group writes as.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// Writes `change` to `path` in the value store, in the group, as
    /// its origin.
    pub fn set(&self, path: &str, change: Change) -> Result<Applied, Refused> {
        self.write(Set::new(path, change, self.origin.clone()))
    }

    /// Writes `set` to the value store, in the group (its gesture added).
    pub fn write(&self, set: Set) -> Result<Applied, Refused> {
        let set = match self.gesture {
            Some(g) => set.gesture(g),
            None => set,
        };
        lock(&self.app.store()).set(set)
    }
}

/// Closes the group: kept when `done` was set, put back otherwise (an
/// error or a panic in the closure).
struct Open<'a> {
    app: &'a AppHandle,
    route: Option<Route>,
    gesture: Option<GestureId>,
    keep: bool,
}

impl Drop for Open<'_> {
    fn drop(&mut self) {
        if let (Some(route), Some(g)) = (self.route.take(), self.gesture) {
            self.app.end_group_by(route, g, self.keep);
        }
    }
}

impl AppHandle {
    /// Runs `f` in a history group labelled `label`: its writes are one
    /// undo step, written as `origin`. The group is ended when `f`
    /// returns `Ok`, cancelled (its writes put back) when it returns
    /// `Err` or panics. It goes through the host
    /// ([`Host::begin_group`](crate::host::Host::begin_group)), or, from
    /// inside a command, through [`Host::groups`](crate::host::Host::groups).
    pub fn group<R, E>(
        &self,
        origin: &Origin,
        label: &str,
        f: impl FnOnce(&Group<'_>) -> Result<R, E>,
    ) -> Result<R, E> {
        let (route, gesture) = self.begin_group_by(origin, label);
        let mut open = Open {
            app: self,
            route: Some(route),
            gesture,
            keep: false,
        };
        let group = Group {
            app: self,
            origin: origin.clone(),
            gesture,
        };
        let out = f(&group);
        open.keep = out.is_ok();
        drop(open);
        out
    }

    fn begin_group_by(&self, origin: &Origin, label: &str) -> (Route, Option<GestureId>) {
        if let Some(g) = self.with_host_mut(|h| h.begin_group(origin, label)) {
            return (Route::Host, g);
        }
        // The host is dispatching (a command's handler runs).
        if let Some(groups) = self.dispatch_groups() {
            let g = groups.begin(origin, label);
            return (Route::Groups(groups), g);
        }
        let g = lock(&self.store()).begin_group(origin.clone(), label);
        (Route::Store, Some(g))
    }

    fn end_group_by(&self, route: Route, group: GestureId, keep: bool) {
        let end_in_store = |app: &AppHandle| {
            let store = app.store();
            let mut s = lock(&store);
            if keep {
                s.end(group);
            } else {
                s.cancel(group);
            }
        };
        match route {
            Route::Host => {
                if self.with_host_mut(|h| h.end_group(group, keep)).is_none() {
                    // Opened through the host, closed while it dispatches:
                    // the store's own group is the default's.
                    end_in_store(self);
                }
            }
            Route::Groups(groups) => groups.end(group, keep),
            Route::Store => end_in_store(self),
        }
    }
}

impl Call<'_> {
    /// Runs `f` in a history group labelled `label`, as the command's
    /// caller: [`AppHandle::group`] with this call's origin.
    pub fn group<R, E>(
        &self,
        label: &str,
        f: impl FnOnce(&Group<'_>) -> Result<R, E>,
    ) -> Result<R, E> {
        self.app.group(self.origin, label, f)
    }
}
