//! The host: the application's kernel, which egui-pop wraps (ADR 0010).
//!
//! egui-pop keeps the interface's part: command declarations (name,
//! keys, `when`, menus) feeding the menus, the palette, the keymap and the
//! traces; focus, overlays, layout, theme and interface settings. The
//! application's part goes through a [`Host`]:
//!
//! - [`Host::dispatch`] runs a command, now or later ([`Reply`]), so a
//!   long or remote command never holds a frame;
//! - an event journal ([`Host::publish`], [`Host::since`],
//!   [`Host::last_seq`]): each event has a sequence number, a source and
//!   a cause, and tests and the debug bridge read it since a sequence
//!   number instead of sleeping. A host keeps it in a [`Journal`] it
//!   hands out ([`Host::journal`]), or on its own bus (it answers
//!   `publish`, `since` and `last_seq` itself), or not at all: egui-pop
//!   then keeps one in memory, as `LocalHost` does;
//! - the value store holding settings ([`Host::store`]);
//! - the application's history: [`Host::undo`] and [`Host::redo`], with
//!   [`Host::can_undo`], [`Host::can_redo`] and the labels the menus show
//!   ("Undo exposure change"). egui-pop owns the keys, the commands and
//!   the steps around them (`edit.undo`, `edit.redo`, ADR 0011); the host
//!   says what an undo is. The default undoes the store;
//! - a [`Waker`] the host calls from any thread when something changed
//!   ([`Host::set_waker`]);
//! - where the workbench's layout is kept ([`Host::layout_document`]):
//!   one document by default, a workspace's own, or none (the host keeps
//!   it);
//! - the owner of the context stack ([`Host::contexts`]): the kernel's,
//!   or egui-pop's `LocalContexts` by default.
//!
//! [`LocalHost`] is the default: commands run in-process on the
//! interface thread (their handlers given to `Ctx::command`), the journal
//! is kept in memory. An application with its own kernel implements
//! `Host`, declares its commands with `Ctx::declare` and gives the host
//! to `AppBuilder::host`.
//!
//! ```
//! use egui_pop_api::host::{self, Dispatch, Host, Journal, Reply, Waker};
//! use egui_pop_api::{App, Ctx, Plugin, PluginError};
//! use egui_pop_model::commands::{CommandSpec, Origin};
//! use egui_pop_model::ids::{CommandId, PluginId};
//!
//! /// A kernel elsewhere: answers later, from another thread.
//! #[derive(Default)]
//! struct Remote {
//!     journal: Journal,
//!     store: egui_pop_core::binding::SharedStore,
//!     waker: Waker,
//! }
//!
//! impl Host for Remote {
//!     fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
//!         let (pending, resolver) = host::pending();
//!         let waker = self.waker.clone();
//!         let n = d.args.as_i64().unwrap_or(0);
//!         std::thread::spawn(move || {
//!             resolver.resolve(Ok(serde_json::json!(n * 2)));
//!             waker.wake();
//!         })
//!         .join()
//!         .unwrap();
//!         Reply::Later(pending)
//!     }
//!     fn journal(&self) -> Option<&Journal> { Some(&self.journal) }
//!     fn journal_mut(&mut self) -> Option<&mut Journal> { Some(&mut self.journal) }
//!     fn store(&self) -> egui_pop_core::binding::SharedStore { self.store.clone() }
//!     fn set_waker(&mut self, waker: Waker) { self.waker = waker; }
//! }
//!
//! struct Doubler;
//! impl Plugin for Doubler {
//!     fn id(&self) -> PluginId { PluginId::from_static("doubler") }
//!     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
//!         ctx.declare(CommandSpec::new(CommandId::from_static("doubler.go"), "Double", "Demo"))?;
//!         Ok(())
//!     }
//! }
//!
//! let mut app = App::builder().host(Remote::default()).plugin(Doubler)
//!     .build(&egui::Context::default()).unwrap();
//! let go = CommandId::from_static("doubler.go");
//! assert!(matches!(app.run(&go, serde_json::json!(21), Origin::User), Reply::Later(_)));
//! assert!(app.handle().running(&go));
//! app.settle();
//! assert_eq!(app.handle().history().last().unwrap().result, Ok(serde_json::json!(42)));
//! ```

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

use egui_pop_core::binding::SharedStore;
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::CommandId;
use egui_pop_model::values::Store;
use egui_pop_model::when::WhenContext;
use serde::Serialize;
use serde_json::Value as Json;

use crate::app::{AppHandle, Call, CommandFn};

/// A position in the journal: events are numbered from 1, in order.
pub type Seq = u64;

/// Something that happened, as the journal keeps it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Event {
    pub seq: Seq,
    /// Who did it: `user`, `agent:<name>`, a plugin, the host.
    pub source: String,
    /// The event it follows from (a command's dispatch for its result).
    pub cause: Option<Seq>,
    /// What happened: `command.dispatched`, `command.done`,
    /// `command.failed`, or the application's own names.
    pub name: String,
    pub data: Json,
}

/// How many events a [`Journal`] keeps by default.
pub const JOURNAL: usize = 1024;

/// Events in order, the oldest dropped past a capacity.
#[derive(Clone, Debug)]
pub struct Journal {
    events: VecDeque<Event>,
    next: Seq,
    capacity: usize,
}

impl Default for Journal {
    fn default() -> Self {
        Self::with_capacity(JOURNAL)
    }
}

impl Journal {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: VecDeque::new(),
            next: 1,
            capacity: capacity.max(1),
        }
    }

    /// Appends an event; returns its sequence number.
    pub fn publish(&mut self, source: &str, cause: Option<Seq>, name: &str, data: Json) -> Seq {
        let seq = self.next;
        self.next += 1;
        if self.events.len() == self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(Event {
            seq,
            source: source.into(),
            cause,
            name: name.into(),
            data,
        });
        seq
    }

    /// The events after `seq` still kept, oldest first (`since(0)`: all).
    pub fn since(&self, seq: Seq) -> Vec<Event> {
        self.events
            .iter()
            .filter(|e| e.seq > seq)
            .cloned()
            .collect()
    }

    /// The last sequence number given (0: none yet).
    pub fn last(&self) -> Seq {
        self.next - 1
    }
}

/// Wakes the interface (asks for a frame) from any thread.
#[derive(Clone)]
pub struct Waker(Arc<dyn Fn() + Send + Sync>);

impl Waker {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }

    pub fn wake(&self) {
        (self.0)();
    }
}

impl Default for Waker {
    /// Wakes nothing (before the application gives its own).
    fn default() -> Self {
        Self::new(|| {})
    }
}

impl fmt::Debug for Waker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Waker")
    }
}

type Slot = Arc<Mutex<Option<Result<Json, String>>>>;

/// A result to come, held by the interface.
#[derive(Clone, Debug)]
pub struct Pending(Slot);

/// Gives a [`Pending`] its result, from any thread.
#[derive(Debug)]
pub struct Resolver(Slot);

/// A result to come and the way to give it.
pub fn pending() -> (Pending, Resolver) {
    let slot: Slot = Arc::default();
    (Pending(slot.clone()), Resolver(slot))
}

impl Pending {
    /// The result, once given (taken once).
    pub fn take(&self) -> Option<Result<Json, String>> {
        lock(&self.0).take()
    }
}

impl Resolver {
    pub fn resolve(self, result: Result<Json, String>) {
        *lock(&self.0) = Some(result);
    }
}

/// What a dispatch gives back.
#[derive(Debug)]
pub enum Reply {
    /// The result, now.
    Done(Result<Json, String>),
    /// The result comes later; the command shows as running until then.
    Later(Pending),
}

impl Reply {
    /// The result if it is there now.
    pub fn now(self) -> Option<Result<Json, String>> {
        match self {
            Reply::Done(r) => Some(r),
            Reply::Later(_) => None,
        }
    }
}

impl PartialEq for Reply {
    /// Two results now compare by value; results to come never equal.
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other), (Reply::Done(a), Reply::Done(b)) if a == b)
    }
}

/// A command handed to the host.
pub struct Dispatch<'a> {
    pub command: &'a CommandId,
    pub args: &'a Json,
    pub origin: &'a Origin,
    /// The context it runs in (its `when` held there).
    pub context: &'a WhenContext,
    /// Its `command.dispatched` event: the cause of its result.
    pub seq: Seq,
    pub app: &'a AppHandle,
    local: Option<&'a CommandFn>,
}

impl fmt::Debug for Dispatch<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dispatch")
            .field("command", self.command)
            .field("args", self.args)
            .field("origin", self.origin)
            .field("seq", &self.seq)
            .field("local", &self.local.is_some())
            .finish_non_exhaustive()
    }
}

impl<'a> Dispatch<'a> {
    pub(crate) fn new(
        command: &'a CommandId,
        args: &'a Json,
        origin: &'a Origin,
        context: &'a WhenContext,
        seq: Seq,
        app: &'a AppHandle,
        local: Option<&'a CommandFn>,
    ) -> Self {
        Self {
            command,
            args,
            origin,
            context,
            seq,
            app,
            local,
        }
    }

    /// Whether a plugin gave this command an in-process handler.
    pub fn has_local(&self) -> bool {
        self.local.is_some()
    }

    /// Runs the in-process handler, if there is one.
    pub fn run_local(&self) -> Option<Result<Json, String>> {
        self.local.map(|h| {
            h(&Call {
                args: self.args,
                origin: self.origin,
                app: self.app,
            })
        })
    }
}

/// The application's kernel, as the interface sees it.
pub trait Host {
    /// Runs a command (its `when` already holds in `d.context`).
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply;

    /// The host's journal, when it keeps one in memory (`LocalHost`
    /// does). `None` (the default): the host answers `publish`, `since`
    /// and `last_seq` from its own bus, or keeps no journal and egui-pop
    /// keeps one.
    fn journal(&self) -> Option<&Journal> {
        None
    }

    /// The host's journal, to append to.
    fn journal_mut(&mut self) -> Option<&mut Journal> {
        None
    }

    /// Appends an event to the host's journal; its sequence number, or
    /// `None` when the host keeps no journal (egui-pop's in-memory one
    /// keeps the event then).
    fn publish(&mut self, source: &str, cause: Option<Seq>, name: &str, data: Json) -> Option<Seq> {
        self.journal_mut()
            .map(|j| j.publish(source, cause, name, data))
    }

    /// The events after `seq`, oldest first; `None` when the host keeps
    /// no journal.
    fn since(&self, seq: Seq) -> Option<Vec<Event>> {
        self.journal().map(|j| j.since(seq))
    }

    /// The last sequence number given (0: none yet); `None` when the host
    /// keeps no journal.
    fn last_seq(&self) -> Option<Seq> {
        self.journal().map(Journal::last)
    }

    /// The value store holding settings and values.
    fn store(&self) -> SharedStore;

    /// Whether there is something to undo (the `canUndo` context key,
    /// greying Edit > Undo). The default asks the store.
    fn can_undo(&self) -> bool {
        !lock(&self.store()).undo_steps().is_empty()
    }

    /// Whether there is something to redo (`canRedo`).
    fn can_redo(&self) -> bool {
        !lock(&self.store()).redo_steps().is_empty()
    }

    /// What an undo would take back, as the menu names it ("exposure
    /// change" for "Undo exposure change"); `None`: no label.
    fn undo_label(&self) -> Option<String> {
        lock(&self.store())
            .undo_steps()
            .last()
            .map(|s| s.label.clone())
    }

    /// What a redo would do again.
    fn redo_label(&self) -> Option<String> {
        lock(&self.store())
            .redo_steps()
            .last()
            .map(|s| s.label.clone())
    }

    /// Undoes the last step of the application's history: the label of
    /// what was undone, `None` when there was nothing. The default undoes
    /// the store. egui-pop calls it from the `edit.undo` step's default
    /// run, outside any dispatch.
    fn undo(&mut self, origin: &Origin) -> Result<Option<String>, String> {
        lock(&self.store()).undo(origin).map_err(|r| r.to_string())
    }

    /// Redoes the last undone step.
    fn redo(&mut self, origin: &Origin) -> Result<Option<String>, String> {
        lock(&self.store()).redo(origin).map_err(|r| r.to_string())
    }

    /// How to wake the interface; the host calls it from any thread when
    /// a result or an event arrives.
    fn set_waker(&mut self, waker: Waker);

    /// The document the workbench's layout is kept in, under the
    /// application's persistence directory (`AppBuilder::persist`):
    /// [`LAYOUT_DOCUMENT`] by default, a workspace's own
    /// (`workspaces/photos/layout`), asked again at each load and save.
    /// `None`: egui-pop neither loads nor saves the layout; the host keeps
    /// it (`AppHandle::layout`, `AppHandle::set_layout`). Loading and
    /// saving are the `layout.load` and `layout.save` steps.
    fn layout_document(&self) -> Option<String> {
        Some(LAYOUT_DOCUMENT.into())
    }

    /// The owner of the context stack (places and modes, ADR 0012), when
    /// the application's kernel keeps it. `None` (the default): egui-pop's
    /// own [`LocalContexts`](crate::contexts::LocalContexts), kept by
    /// `ContextsPlugin`. The interface only reads it and asks it for
    /// transitions; it never writes the stack.
    fn contexts(&mut self) -> Option<&mut dyn crate::contexts::Contexts> {
        None
    }
}

/// The layout's document when the host names none of its own.
pub const LAYOUT_DOCUMENT: &str = "layout";

/// The default host: commands run in-process with their handlers, the
/// journal is in memory.
#[derive(Debug)]
pub struct LocalHost {
    journal: Journal,
    store: SharedStore,
    waker: Waker,
    layout: Option<String>,
}

impl Default for LocalHost {
    fn default() -> Self {
        Self::new(Arc::new(Mutex::new(Store::new())))
    }
}

impl LocalHost {
    /// A local host over a store.
    pub fn new(store: SharedStore) -> Self {
        Self {
            journal: Journal::default(),
            store,
            waker: Waker::default(),
            layout: Some(LAYOUT_DOCUMENT.into()),
        }
    }

    /// Where the layout is kept (see [`Host::layout_document`]); `None`:
    /// the application keeps it itself.
    pub fn with_layout_document(mut self, document: Option<&str>) -> Self {
        self.layout = document.map(Into::into);
        self
    }

    /// The waker the application gave (to wake it from a thread).
    pub fn waker(&self) -> Waker {
        self.waker.clone()
    }
}

impl Host for LocalHost {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        Reply::Done(
            d.run_local()
                .unwrap_or_else(|| Err(format!("no handler for `{}` in this host", d.command))),
        )
    }

    fn journal(&self) -> Option<&Journal> {
        Some(&self.journal)
    }

    fn journal_mut(&mut self) -> Option<&mut Journal> {
        Some(&mut self.journal)
    }

    fn store(&self) -> SharedStore {
        self.store.clone()
    }

    fn set_waker(&mut self, waker: Waker) {
        self.waker = waker;
    }

    fn layout_document(&self) -> Option<String> {
        self.layout.clone()
    }
}

/// The journal's name for an origin: `user`, `agent:<session>`,
/// `script:<name>`...
pub fn source_of(origin: &Origin) -> String {
    match origin {
        Origin::User => "user".into(),
        Origin::Agent(n) => format!("agent:{n}"),
        Origin::Script(n) => format!("script:{n}"),
        Origin::Device(n) => format!("device:{n}"),
        Origin::Remote(n) => format!("remote:{n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_journal_is_read_since_a_sequence_number() {
        let mut j = Journal::with_capacity(3);
        assert_eq!(j.last(), 0);
        let a = j.publish("user", None, "a", Json::Null);
        let b = j.publish("host", Some(a), "b", Json::Null);
        assert_eq!((a, b), (1, 2));
        assert_eq!(j.since(a).len(), 1);
        assert_eq!(j.since(a)[0].cause, Some(1));
        for n in 0..3 {
            j.publish("x", None, &format!("{n}"), Json::Null);
        }
        // Past the capacity, the oldest go; numbers keep counting.
        assert_eq!(j.since(0).first().map(|e| e.seq), Some(3));
        assert_eq!(j.last(), 5);
    }

    #[test]
    fn a_result_can_come_from_another_thread() {
        let (p, r) = pending();
        assert_eq!(p.take(), None);
        std::thread::spawn(move || r.resolve(Ok(Json::Bool(true))))
            .join()
            .unwrap();
        assert_eq!(p.take(), Some(Ok(Json::Bool(true))));
        assert_eq!(p.take(), None);
        assert_eq!(source_of(&Origin::User), "user");
    }
}
