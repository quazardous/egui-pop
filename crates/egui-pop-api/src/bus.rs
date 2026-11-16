//! The bus: requests, events and the listeners of each, joined
//! ([ADR 0029](https://github.com/quazardous/egui-pop/blob/main/docs/adr/0029-not-on-the-bus.md)).
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! A module knows the bus, not its neighbours. A state change another
//! part may care about is a **request**, asked with who asks and why (a
//! step, ADR 0011, named `<noun>.<verb>`), and an **event** said once it
//! is done or refused (the journal, `<noun>.<past>`). Whoever cares
//! listens, under a name and at a place it declares, in the hook kernel
//! the application holds ([`AppHandle::bus`]); nobody calls a neighbour to
//! tell it.
//!
//! The window lifecycle is the first family on it. Its points
//! ([`WindowOpen`], [`WindowClose`], [`WindowReparent`], [`PopupOpen`],
//! [`PopupClose`]) take a [`WindowRequest`]; the registry
//! ([`REGISTRY`]) listens to them and is the one list of what is on
//! screen ([`AppHandle::windows_list`]). Closing an application window
//! ([`AppHandle::request_window_close`]) runs, in one call and in this
//! order:
//!
//! 1. [`CHILDREN`]: what belongs to the window goes with it (its dialogs);
//! 2. the owner's listener (`admin.window` for the Settings app): the
//!    state the window showed closes;
//! 3. [`FOCUS`]: the window worked in before becomes the active one;
//! 4. [`REGISTRY`]: the window is closed, no longer declared to the
//!    backend, hidden where the system allows;
//! 5. [`PLACE`]: its place is kept (written to disk later);
//! 6. [`INVARIANTS`]: last, checks the registry agrees.
//!
//! `window.closed` is said after the last of them, so whoever reads it
//! finds the owner's state, the focus and the registry already agree.
//! [`order_dump`] prints who runs when; the `bus` collector of the debug
//! bridge shows it too.
//!
//! The Settings app's window is wholly on the bus: the app asks
//! `window.open` and `window.close` for it as its owner, in the call that
//! changed what it wants, and its listener (`admin.window`) keeps the
//! app's state with the window on both points. A plugin disconnected has
//! its windows closed ([`OWNER_GONE`], reason `owner-gone`).
//!
//! The shell's windows are on the bus too: opening an application window
//! is its `window.open` ([`AppHandle::request_window_open`], the
//! application as owner for [`AppHandle::open_window`]); the main
//! window's close, whatever asked (the system, its title bar's Close,
//! [`AppHandle::close_window`], [`AppHandle::quit`] with the reason
//! `quit`), is its `window.close`, which closes every other window by its
//! own `window.close` first; a window's content moving into the main
//! window when it closes, and back when the main window is asked to its
//! front, is a `window.reparent` ([`AppHandle::request_window_reparent`]).
//! Detached panels and popups still do their work themselves and report
//! it to the registry; they move onto their requests one by one (the
//! ADR's slices).
//!
//! The job lifecycle is the second family. Asking about a job is a
//! request ([`JobCancel`], [`JobPause`], [`JobResume`], [`JobRetry`], each
//! a [`JobAsk`], asked by [`AppHandle::request_job`]); whoever runs the
//! job does the work and answers with its `job.*` events. Those events,
//! and the job commands' `command.*` events, are delivered to the
//! listeners of [`JobEvents`] (`job.*`) in journal order, once each
//! ([`AppHandle::deliver_events`]): the jobs plugin's model is one of them
//! (`jobs.model`), so no job state changes but on the bus. A plugin
//! connecting or disconnecting is asked on [`PluginConnect`] and
//! [`PluginDisconnect`] (a [`PluginChange`]), the disconnection before
//! `plugin.disconnected` is said: the jobs a plugin leaves are cancelled
//! or said orphaned by a listener of its disconnection, and taken back by
//! one of its connection.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};

use egui::{ViewportCommand, ViewportId};
use egui_pop_core::lock;
use egui_pop_hooks::{Kernel, ListenOpts, Point, PointSpec};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{AppHandle, WeakApp};
use crate::host::{Event, Seq};
use crate::windows::WindowId;

pub use egui_pop_hooks;

mod owners;
pub use owners::OWNER_GONE;
mod shell;
pub use shell::STEP_WINDOW_REPARENT;

/// Why a window or a popup opens or closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Reason {
    /// The person: a title bar's close, a Close button, Escape.
    User,
    /// A command or the application's code.
    Command,
    /// Its parent window closed.
    ParentClosed,
    /// Its owner went (a plugin disconnected, the state it showed closed).
    OwnerGone,
    /// The paint guard (a window that painted nothing, an orphan).
    Guard,
    /// The system (the backend dropped it).
    System,
    /// The application quits.
    Quit,
}

impl Reason {
    /// The reason as the journal writes it (`parent-closed`).
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::User => "user",
            Reason::Command => "command",
            Reason::ParentClosed => "parent-closed",
            Reason::OwnerGone => "owner-gone",
            Reason::Guard => "guard",
            Reason::System => "system",
            Reason::Quit => "quit",
        }
    }
}

/// What a window of the registry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum WindowKind {
    /// The main window (the root viewport).
    Main,
    /// An application window ([`AppHandle::open_window`]).
    App,
    /// A detached panel's window.
    Panel,
    /// A native window egui does not paint (a system file chooser, a web
    /// view).
    Native,
    /// A popup inside or over a window.
    Popup,
}

/// A window or popup request: which, of what kind, who asks, why, and
/// the event that caused it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WindowRequest {
    /// The window's id ([`WindowId`]), or the popup's.
    pub window: String,
    pub kind: WindowKind,
    /// Who asks: a plugin's id, a command's, a parent window's.
    pub owner: String,
    pub reason: Reason,
    /// The journal sequence number of the event that caused it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<u64>,
    /// The parent window, for a child window or a popup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl WindowRequest {
    pub fn new(
        window: impl Into<String>,
        kind: WindowKind,
        owner: impl Into<String>,
        reason: Reason,
    ) -> Self {
        Self {
            window: window.into(),
            kind,
            owner: owner.into(),
            reason,
            cause: None,
            parent: None,
        }
    }

    /// The journal event that caused it.
    pub fn cause(mut self, seq: u64) -> Self {
        self.cause = Some(seq);
        self
    }

    /// Its parent window.
    pub fn parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }
}

/// A window opens (`window.open`); its listeners say what they did, one
/// line each, in the value.
#[derive(Debug)]
pub struct WindowOpen;
impl Point for WindowOpen {
    const ID: &'static str = "window.open";
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A window closes (`window.close`).
#[derive(Debug)]
pub struct WindowClose;
impl Point for WindowClose {
    const ID: &'static str = "window.close";
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A window's content moves to another window (`window.reparent`).
#[derive(Debug)]
pub struct WindowReparent;
impl Point for WindowReparent {
    const ID: &'static str = "window.reparent";
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A popup opens in a window (`popup.open`).
#[derive(Debug)]
pub struct PopupOpen;
impl Point for PopupOpen {
    const ID: &'static str = "popup.open";
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A popup closes (`popup.close`).
#[derive(Debug)]
pub struct PopupClose;
impl Point for PopupClose {
    const ID: &'static str = "popup.close";
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// The window and popup points, in the order [`order_dump`] prints them.
pub const WINDOW_POINTS: [&str; 5] = [
    WindowOpen::ID,
    WindowClose::ID,
    WindowReparent::ID,
    PopupOpen::ID,
    PopupClose::ID,
];

/// A request about a job, asked of whoever runs it (the host, or the
/// application's own runner): which job, what, and why when it is not
/// the person asking.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct JobAsk {
    /// The job's id.
    pub id: String,
    pub request: egui_pop_model::jobs::Request,
    /// Why, when it is not the person asking (its plugin disconnected).
    pub reason: Option<String>,
    /// Asked by the framework itself, whoever acts now: its command runs
    /// as `script:egui_pop`.
    pub(crate) framework: bool,
}

impl JobAsk {
    pub fn new(id: impl Into<String>, request: egui_pop_model::jobs::Request) -> Self {
        Self {
            id: id.into(),
            request,
            reason: None,
            framework: false,
        }
    }

    /// Why it is asked.
    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// The framework's own request (the jobs of a plugin that went).
    pub(crate) fn by_framework(mut self) -> Self {
        self.framework = true;
        self
    }

    /// Whether the framework itself asks it.
    pub fn is_framework(&self) -> bool {
        self.framework
    }

    /// The command's arguments: `{ "id" }`, with `reason` when given.
    pub fn args(&self) -> serde_json::Value {
        let mut args = json!({ "id": self.id });
        if let Some(r) = &self.reason {
            args["reason"] = json!(r);
        }
        args
    }
}

/// A job is asked to stop (`job.cancel`); its listeners say what they
/// did, one line each, in the value.
#[derive(Debug)]
pub struct JobCancel;
impl Point for JobCancel {
    const ID: &'static str = "job.cancel";
    type Input = JobAsk;
    type Value = Vec<String>;
}

/// A job is asked to pause (`job.pause`).
#[derive(Debug)]
pub struct JobPause;
impl Point for JobPause {
    const ID: &'static str = "job.pause";
    type Input = JobAsk;
    type Value = Vec<String>;
}

/// A paused job is asked to go on (`job.resume`).
#[derive(Debug)]
pub struct JobResume;
impl Point for JobResume {
    const ID: &'static str = "job.resume";
    type Input = JobAsk;
    type Value = Vec<String>;
}

/// A failed or cancelled job is asked to run again (`job.retry`).
#[derive(Debug)]
pub struct JobRetry;
impl Point for JobRetry {
    const ID: &'static str = "job.retry";
    type Input = JobAsk;
    type Value = Vec<String>;
}

/// A job event was said (`job.*`): each `job.*` event of the journal, and
/// each `command.*` event of a job command, delivered once, in journal
/// order ([`AppHandle::deliver_events`]).
#[derive(Debug)]
pub struct JobEvents;
impl Point for JobEvents {
    const ID: &'static str = "job.*";
    type Input = Event;
    type Value = ();
}

/// A plugin connecting or disconnecting: which, its owner id, why, and
/// the commands it registered (gone with it on a disconnection).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PluginChange {
    pub plugin: String,
    pub owner: String,
    /// Why: `connected`, `removed`, `failed connect`…
    pub reason: String,
    pub commands: Vec<String>,
}

impl PluginChange {
    pub fn new(
        plugin: impl Into<String>,
        owner: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            plugin: plugin.into(),
            owner: owner.into(),
            reason: reason.into(),
            commands: Vec::new(),
        }
    }

    /// The commands it registered.
    pub fn commands(mut self, commands: Vec<String>) -> Self {
        self.commands = commands;
        self
    }
}

/// A plugin is connected (`plugin.connect`), what it registers in
/// place; its listeners take back what it had left.
#[derive(Debug)]
pub struct PluginConnect;
impl Point for PluginConnect {
    const ID: &'static str = "plugin.connect";
    type Input = PluginChange;
    type Value = Vec<String>;
}

/// A plugin is disconnected (`plugin.disconnect`), what it registered
/// removed, before `plugin.disconnected` is said: each listener adds what
/// it did to the value (`jobs_cancelled`…), which the event's `removed`
/// carries.
#[derive(Debug)]
pub struct PluginDisconnect;
impl Point for PluginDisconnect {
    const ID: &'static str = "plugin.disconnect";
    type Input = PluginChange;
    type Value = serde_json::Map<String, serde_json::Value>;
}

/// The job points, in the order [`order_dump`] prints them.
pub const JOB_POINTS: [&str; 5] = [
    JobCancel::ID,
    JobPause::ID,
    JobResume::ID,
    JobRetry::ID,
    JobEvents::ID,
];

/// The plugin points, in the order [`order_dump`] prints them.
pub const PLUGIN_POINTS: [&str; 2] = [PluginConnect::ID, PluginDisconnect::ID];

/// Every point egui-pop declares: the window, job and plugin points.
pub const POINTS: [&str; 12] = [
    WindowOpen::ID,
    WindowClose::ID,
    WindowReparent::ID,
    PopupOpen::ID,
    PopupClose::ID,
    JobCancel::ID,
    JobPause::ID,
    JobResume::ID,
    JobRetry::ID,
    JobEvents::ID,
    PluginConnect::ID,
    PluginDisconnect::ID,
];

/// What owns egui-pop's own listeners.
pub const OWNER: &str = "egui_pop";
/// What belongs to a closing window goes with it: its dialogs.
pub const CHILDREN: &str = "window.children";
/// The window worked in before a closing one becomes the active one.
pub const FOCUS: &str = "window.focus";
/// The one list of windows and popups ([`AppHandle::windows_list`]).
pub const REGISTRY: &str = "window.registry";
/// A window's place, kept when it closes.
pub const PLACE: &str = "window.place";
/// Checks, last, the window invariants ([`crate::window_invariants`]).
pub const INVARIANTS: &str = "window.invariants";

/// A window or popup as the registry holds it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct WindowRecord {
    pub id: String,
    pub kind: WindowKind,
    pub owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Its viewport, as egui's debug text names it (`9FA2`), when it has
    /// one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
    /// The reason of the request that opened it.
    pub opened_because: Reason,
}

/// The registry's state: the windows and popups open, the main window
/// first, then in opening order.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    records: Vec<WindowRecord>,
}

impl Registry {
    fn open(&mut self, req: &WindowRequest, viewport: Option<ViewportId>) {
        self.records.retain(|r| r.id != req.window);
        self.records.push(WindowRecord {
            id: req.window.clone(),
            kind: req.kind,
            owner: req.owner.clone(),
            parent: req.parent.clone(),
            viewport: viewport.map(|v| format!("{v:?}")),
            opened_because: req.reason,
        });
    }

    fn close(&mut self, id: &str) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.id != id);
        before != self.records.len()
    }

    fn holds(&self, id: &str) -> bool {
        self.records.iter().any(|r| r.id == id)
    }
}

/// The bus an application holds: the kernel and the window registry.
pub(crate) struct Bus {
    pub(crate) kernel: Arc<Kernel>,
    pub(crate) registry: Arc<Mutex<Registry>>,
    /// What the window invariants remember between passes
    /// ([`crate::window_invariants`]).
    pub(crate) invariants: Mutex<crate::window_invariants::State>,
    /// The last journal event delivered to the event points' listeners.
    delivered: Cell<Seq>,
    /// A delivery runs: one asked from inside it is left to it.
    delivering: Cell<bool>,
}

impl std::fmt::Debug for Bus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bus").finish_non_exhaustive()
    }
}

impl Default for Bus {
    fn default() -> Self {
        let kernel = Arc::new(Kernel::new());
        let mut main = Registry::default();
        main.open(
            &WindowRequest::new(WindowId::MAIN, WindowKind::Main, "app", Reason::System),
            Some(ViewportId::ROOT),
        );
        let registry = Arc::new(Mutex::new(main));
        install(&kernel, &registry);
        Bus {
            kernel,
            registry,
            invariants: Mutex::default(),
            delivered: Cell::new(0),
            delivering: Cell::new(false),
        }
    }
}

thread_local! {
    /// The applications whose request is running, innermost last: what a
    /// listener acts on. Listeners run inline, on the thread that asked.
    static ASKING: RefCell<Vec<WeakApp>> = const { RefCell::new(Vec::new()) };
}

/// The application whose request runs now, for a listener (listeners
/// are `Send + Sync` and keep no handle).
pub(crate) fn asking() -> Option<AppHandle> {
    ASKING.with_borrow(|a| a.last().and_then(|w| w.get().ok()))
}

/// Takes the application off [`ASKING`] when the request ends, even by a
/// panic a listener's boundary did not catch.
struct Asking;

impl Drop for Asking {
    fn drop(&mut self) {
        ASKING.with_borrow_mut(|a| a.pop());
    }
}

/// Runs `P`'s listeners for `app`, inline: what they did, in order, and
/// their failures logged.
fn emit<P: Point<Input = WindowRequest, Value = Vec<String>>>(
    app: &AppHandle,
    req: WindowRequest,
) -> Vec<String> {
    emit_with::<P>(app, req, Vec::new())
}

/// Runs `P`'s listeners for `app`, inline, from `value`: what they left,
/// their failures logged.
fn emit_with<P: Point>(app: &AppHandle, input: P::Input, value: P::Value) -> P::Value
where
    P::Value: Clone,
{
    ASKING.with_borrow_mut(|a| a.push(app.downgrade()));
    let _asking = Asking;
    let out = app.0.bus.kernel.alter::<P>(input, value);
    for f in &out.failures {
        log::error!(target: "egui_pop", "{f}");
    }
    out.value
}

/// Whether a journal event goes to [`JobEvents`]: a `job.*` event, or a
/// job command's `command.*` event.
pub(crate) fn is_job_event(e: &Event) -> bool {
    e.name.starts_with("job.")
        || (e.name.starts_with("command.")
            && e.data["command"]
                .as_str()
                .is_some_and(|c| c.starts_with("job.")))
}

/// Ends a delivery, even by a panic a listener's boundary did not catch.
struct Delivering<'a>(&'a Cell<bool>);

impl Drop for Delivering<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Declares the window points and registers egui-pop's listeners.
fn install(kernel: &Kernel, registry: &Arc<Mutex<Registry>>) {
    let declare = [
        (
            WindowOpen::ID,
            "A window opens: its id, kind, owner, parent.",
        ),
        (
            WindowClose::ID,
            "A window closes, whoever asked: its children, its owner's state, the focus, the registry and its place follow, in one call.",
        ),
        (
            WindowReparent::ID,
            "A window's content moves to another window (into the main window and back).",
        ),
        (PopupOpen::ID, "A popup opens in a window."),
        (PopupClose::ID, "A popup closes."),
    ];
    for (id, description) in declare {
        let spec = PointSpec::new(description);
        let done = match id {
            WindowOpen::ID => kernel.declare::<WindowOpen>(spec),
            WindowClose::ID => kernel.declare::<WindowClose>(spec),
            WindowReparent::ID => kernel.declare::<WindowReparent>(spec),
            PopupOpen::ID => kernel.declare::<PopupOpen>(spec),
            _ => kernel.declare::<PopupClose>(spec),
        };
        debug_assert!(done.is_ok(), "{id} declared once");
    }
    let declared = [
        kernel.declare::<JobCancel>(PointSpec::new(
            "A job is asked to stop: whoever runs it is asked, and the job shows as asked until it answers.",
        )),
        kernel.declare::<JobPause>(PointSpec::new("A job is asked to pause.")),
        kernel.declare::<JobResume>(PointSpec::new("A paused job is asked to go on.")),
        kernel.declare::<JobRetry>(PointSpec::new(
            "A failed or cancelled job is asked to run again.",
        )),
        kernel.declare::<JobEvents>(PointSpec::new(
            "A `job.*` event, or a job command's `command.*` event, was said: delivered once each, in journal order.",
        )),
        kernel.declare::<PluginConnect>(PointSpec::new(
            "A plugin is connected: what it had left is taken back.",
        )),
        kernel.declare::<PluginDisconnect>(PointSpec::new(
            "A plugin is disconnected, before `plugin.disconnected` is said: what it leaves is closed or marked, each listener counting what it did.",
        )),
    ];
    for d in declared {
        debug_assert!(d.is_ok(), "{d:?}");
    }
    let mine = || ListenOpts::new().owner(OWNER);
    // `window.close`, in the ADR's order.
    let listened = [
        kernel.listen::<WindowClose>(CHILDREN, mine().before(FOCUS).priority(1000), |req, did| {
            let Some(app) = asking() else { return Ok(()) };
            if req.kind == WindowKind::Main {
                shell::close_children_of_main(&app, req, did);
            } else if req.kind == WindowKind::App
                && let Some(v) = app.window_viewport(&WindowId::new(&req.window))
            {
                crate::dialogs::window_closed(&app, v);
                did.push(format!("{CHILDREN}: its dialogs cancelled"));
            }
            Ok(())
        }),
        kernel.listen::<WindowClose>(FOCUS, mine().before(REGISTRY), |req, did| {
            let Some(app) = asking() else { return Ok(()) };
            if req.kind == WindowKind::App && app.focus_before_close(&WindowId::new(&req.window)) {
                did.push(format!("{FOCUS}: the window worked in before is active"));
            }
            Ok(())
        }),
        {
            let registry = registry.clone();
            kernel.listen::<WindowClose>(REGISTRY, mine().after(FOCUS), move |req, did| {
                let held = lock(&registry).close(&req.window);
                if let Some(app) = asking() {
                    match req.kind {
                        WindowKind::App => app.registry_close(&WindowId::new(&req.window)),
                        WindowKind::Main if held => app.registry_close_main(),
                        _ => {}
                    }
                }
                if held {
                    did.push(format!("{REGISTRY}: closed"));
                }
                Ok(())
            })
        },
        kernel.listen::<WindowClose>(PLACE, mine().after(REGISTRY), |req, did| {
            let Some(app) = asking() else { return Ok(()) };
            if req.kind == WindowKind::App && app.keep_closing_place(&WindowId::new(&req.window)) {
                did.push(format!("{PLACE}: its place kept"));
            }
            Ok(())
        }),
        {
            let registry = registry.clone();
            kernel.listen::<WindowClose>(INVARIANTS, mine().last(), move |req, did| {
                let left = lock(&registry).holds(&req.window);
                let mut rest = true;
                if let Some(app) = asking() {
                    if left {
                        app.invariant_broken(crate::window_invariants::closed_left(req));
                    }
                    rest = app.check_invariants_after_request();
                }
                if left {
                    return Err(format!("`{}` is still in the registry", req.window).into());
                }
                did.push(format!(
                    "{INVARIANTS}: {}",
                    if rest { "held" } else { "broken" }
                ));
                Ok(())
            })
        },
    ];
    for l in listened {
        debug_assert!(l.is_ok(), "{l:?}");
    }
    // The paths not moved yet report to the registry through their points.
    let registry_open = |registry: Arc<Mutex<Registry>>| {
        move |req: &WindowRequest, did: &mut Vec<String>| {
            let viewport = crate::window_invariants::viewport_of(req.kind, &req.window);
            lock(&registry).open(req, viewport);
            did.push(format!("{REGISTRY}: holds it"));
            Ok(())
        }
    };
    let _ = kernel.listen::<WindowOpen>(REGISTRY, mine(), registry_open(registry.clone()));
    let _ = kernel.listen::<PopupOpen>(REGISTRY, mine(), registry_open(registry.clone()));
    {
        let registry = registry.clone();
        let _ = kernel.listen::<PopupClose>(REGISTRY, mine(), move |req, did| {
            if lock(&registry).close(&req.window) {
                did.push(format!("{REGISTRY}: closed"));
            }
            Ok(())
        });
    }
    shell::install(kernel, registry);
    owners::install(kernel);
    // The invariants check runs last on every other window point: the
    // rules the registry alone answers (the backend's and the paint
    // guard's are checked in the main window's pass).
    let check = |_: &WindowRequest, did: &mut Vec<String>| {
        let held = asking().is_none_or(|app| app.check_invariants_after_request());
        did.push(format!(
            "{INVARIANTS}: {}",
            if held { "held" } else { "broken" }
        ));
        Ok(())
    };
    let others = [
        kernel.listen::<WindowOpen>(INVARIANTS, mine().last(), check),
        kernel.listen::<WindowReparent>(INVARIANTS, mine().last(), check),
        kernel.listen::<PopupOpen>(INVARIANTS, mine().last(), check),
        kernel.listen::<PopupClose>(INVARIANTS, mine().last(), check),
    ];
    for l in others {
        debug_assert!(l.is_ok(), "{l:?}");
    }
}

/// The order the listeners of `points` run in, one point after the other,
/// each listener with its owner and what it declares: "who runs when",
/// read rather than remembered.
pub fn order_dump(kernel: &Kernel, points: &[&str]) -> String {
    let mut out = String::new();
    for point in points {
        out.push_str(point);
        out.push('\n');
        for (n, l) in kernel
            .listeners(point)
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let mut declares: Vec<String> = Vec::new();
            if l.after.iter().any(|a| a == egui_pop_hooks::LAST) {
                declares.push("last".into());
            }
            let after: Vec<&str> = l
                .after
                .iter()
                .filter(|a| *a != egui_pop_hooks::LAST)
                .map(String::as_str)
                .collect();
            if !after.is_empty() {
                declares.push(format!("after {}", after.join(", ")));
            }
            if !l.before.is_empty() {
                declares.push(format!("before {}", l.before.join(", ")));
            }
            if l.priority != 0 {
                declares.push(format!("priority {}", l.priority));
            }
            if !l.missing.is_empty() {
                declares.push(format!("MISSING {}", l.missing.join(", ")));
            }
            let declares = if declares.is_empty() {
                "nothing".to_string()
            } else {
                declares.join("; ")
            };
            out.push_str(&format!(
                "  {}. {} (owner {}): {declares}\n",
                n + 1,
                l.id,
                l.owner.as_deref().unwrap_or("-"),
            ));
        }
    }
    out
}

/// The ordering constraints of `points` that name a listener not there:
/// `point: listener -> target`. A test ends with none.
pub fn missing_targets(kernel: &Kernel, points: &[&str]) -> Vec<String> {
    points
        .iter()
        .flat_map(|p| {
            kernel
                .listeners(p)
                .unwrap_or_default()
                .into_iter()
                .flat_map(move |l| {
                    l.missing
                        .iter()
                        .map(|t| format!("{p}: {} -> {t}", l.id))
                        .collect::<Vec<_>>()
                })
        })
        .collect()
}

impl AppHandle {
    /// The application's bus: the hook kernel its requests' listeners are
    /// registered in (ADR 0029). A part that cares about a window closing
    /// listens to [`WindowClose`] under a name of its own, declaring its
    /// place (`after` / `before` the listeners of this module).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn bus(&self) -> Arc<Kernel> {
        self.0.bus.kernel.clone()
    }

    /// The windows and popups on screen, as the registry holds them: the
    /// main window first, then in opening order (`windows.list`).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn windows_list(&self) -> Vec<WindowRecord> {
        lock(&self.0.bus.registry).records.clone()
    }

    /// The order the listeners of the window and popup points run in
    /// ([`order_dump`]).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn bus_order(&self) -> String {
        order_dump(&self.0.bus.kernel, &WINDOW_POINTS)
    }

    /// Asks for an application window's close (`window.close`): its
    /// listeners all run now, in their declared order, the owner's state
    /// closing with it; `window.closed` is said after the last one. A
    /// before-hook on the `window.close` step may veto it, and a window
    /// not open is refused: both said as `window.refused`. Whether it
    /// closed.
    ///
    /// The system window itself goes at the main window's next pass,
    /// which no longer declares it; a pass of it in between draws its
    /// last page.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_window_close(&self, request: WindowRequest) -> bool {
        let id = WindowId::new(&request.window);
        let open = match request.kind {
            WindowKind::App => self.windows_ref().entry(&id).is_some(),
            WindowKind::Main => lock(&self.0.bus.registry).holds(WindowId::MAIN),
            _ => true,
        };
        if !open {
            self.publish(
                "egui_pop",
                None,
                "window.refused",
                json!({ "request": WindowClose::ID, "window": request.window, "owner": request.owner, "reason": "no open window of this id" }),
            );
            return false;
        }
        let args = serde_json::to_value(&request).unwrap_or_default();
        let outcome = self.step(&STEP_WINDOW_CLOSE, args, |app, args| {
            // A before-hook may have redirected it.
            let req = serde_json::from_value::<WindowRequest>(args.clone())
                .map_err(|e| format!("not a window request: {e}"))?;
            // The main window's close with another window open moves
            // that window into it (ADR 0025): a `window.reparent`.
            if let Some(moved) = shell::move_in_instead(app, &req) {
                return Ok(moved);
            }
            let did = emit::<WindowClose>(app, req);
            Ok(json!({ "did": did }))
        });
        match outcome {
            crate::steps::Outcome::Vetoed { by, reason } => {
                self.publish(
                    "egui_pop",
                    None,
                    "window.refused",
                    json!({ "request": WindowClose::ID, "window": request.window, "owner": request.owner, "vetoed_by": by, "reason": reason }),
                );
                false
            }
            crate::steps::Outcome::Done(Ok(moved)) if moved.get(shell::MOVED_IN).is_some() => {
                // It did not close: the window moved in instead.
                false
            }
            crate::steps::Outcome::Done(result) => {
                let mut data = json!({
                    "window": request.window,
                    "kind": request.kind,
                    "owner": request.owner,
                    "reason": request.reason,
                });
                if let Some(c) = request.cause {
                    data["cause"] = json!(c);
                }
                if let Err(e) = &result {
                    data["error"] = json!(e);
                }
                self.publish("egui_pop", request.cause, "window.closed", data);
                result.is_ok()
            }
        }
    }

    /// The order the listeners of the job and plugin points run in
    /// ([`order_dump`]).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn bus_order_of(&self, points: &[&str]) -> String {
        order_dump(&self.0.bus.kernel, points)
    }

    /// Asks about a job (`job.cancel`, `job.pause`, `job.resume`,
    /// `job.retry`): its listeners run now, in their declared order (the
    /// jobs plugin's: the job shows as asked, then whoever runs it is
    /// asked); the answer comes as the runner's `job.*` events. What they
    /// did, one line each; with nobody listening (no jobs plugin) it is
    /// refused, said as `job.refused`.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_job(&self, ask: JobAsk) -> Vec<String> {
        use egui_pop_model::jobs::Request;
        let point = ask.request.command();
        let id = ask.id.clone();
        let did = match ask.request {
            Request::Cancel => emit_with::<JobCancel>(self, ask, Vec::new()),
            Request::Pause => emit_with::<JobPause>(self, ask, Vec::new()),
            Request::Resume => emit_with::<JobResume>(self, ask, Vec::new()),
            Request::Retry => emit_with::<JobRetry>(self, ask, Vec::new()),
        };
        if did.is_empty() {
            log::warn!(target: "egui_pop", "`{point}` of job `{id}`: nobody listens (no jobs plugin)");
            self.publish(
                "egui_pop",
                None,
                "job.refused",
                json!({ "request": point, "id": id, "reason": "nobody listens to it (no jobs plugin)" }),
            );
        }
        did
    }

    /// Delivers the journal's events not delivered yet to the event
    /// points' listeners ([`JobEvents`]), in journal order, once each.
    /// Each pass does it before its handlers run; a read that must see
    /// every event up to the last (a wait, a plugin going) does it first.
    /// Asked from inside a delivery, it returns at once: the delivery
    /// running goes on, and what it did not take waits for the next.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn deliver_events(&self) {
        let bus = &self.0.bus;
        if bus.delivering.replace(true) {
            return;
        }
        let _delivering = Delivering(&bus.delivering);
        let events = self.events_since(bus.delivered.get());
        for e in events {
            bus.delivered.set(e.seq);
            if is_job_event(&e) {
                emit_with::<JobEvents>(self, e, ());
            }
        }
    }

    /// The last journal event delivered ([`AppHandle::deliver_events`]):
    /// a listener registering late reads the record up to it.
    pub(crate) fn delivered_event(&self) -> Seq {
        self.0.bus.delivered.get()
    }

    /// Asks the plugin points' listeners about a plugin connected: what
    /// they did.
    pub(crate) fn plugin_connected(&self, change: PluginChange) -> Vec<String> {
        self.deliver_events();
        emit_with::<PluginConnect>(self, change, Vec::new())
    }

    /// Asks the plugin points' listeners about a plugin disconnected,
    /// before `plugin.disconnected` is said: what they did, counted.
    pub(crate) fn plugin_disconnected(
        &self,
        change: PluginChange,
    ) -> serde_json::Map<String, serde_json::Value> {
        // Up to the last event: a job started since the last pass counts.
        self.deliver_events();
        emit_with::<PluginDisconnect>(self, change, serde_json::Map::new())
    }

    /// A path not moved onto its request yet reports a window opened, so
    /// the registry holds it (the detached panels' windows, until their
    /// slice).
    pub(crate) fn report_window(&self, point: &str, req: WindowRequest) {
        match point {
            WindowOpen::ID => {
                emit::<WindowOpen>(self, req);
            }
            WindowReparent::ID => {
                emit::<WindowReparent>(self, req);
            }
            PopupOpen::ID => {
                emit::<PopupOpen>(self, req);
            }
            PopupClose::ID => {
                emit::<PopupClose>(self, req);
            }
            _ => {
                // A close goes through `request_window_close`; this one
                // only takes the registry's record away.
                lock(&self.0.bus.registry).close(&req.window);
            }
        }
    }

    /// `window.registry`'s part on an application window: marked closed,
    /// so the main window's next pass no longer declares its viewport and
    /// the backend lets its system window go; hidden at once where the
    /// system allows it; forgotten by the window shim, the paint guard and
    /// the overlays; the main window asked for that pass.
    fn registry_close(&self, id: &WindowId) {
        let found = {
            let mut w = self.windows_mut();
            match w.entries.iter_mut().find(|e| &e.id == id && !e.closed) {
                Some(e) => {
                    e.closed = true;
                    Some(e.title.clone())
                }
                None => None,
            }
        };
        let Some(title) = found else { return };
        let viewport = id.deferred_viewport();
        let ctx = self.egui();
        if !ctx.embed_viewports() {
            // Its passes until the backend drops it draw its last page.
            crate::paint_guard::closing(viewport, title);
            ctx.send_viewport_cmd_to(viewport, ViewportCommand::Visible(false));
        }
        crate::windows::forget_viewport(viewport);
        self.app_window_closed(viewport, id.as_str());
        self.sync_separate_windows();
        ctx.request_repaint_of(ViewportId::ROOT);
    }
}

/// The `window.close` step: the request as steps trace it (before-hooks
/// may veto or redirect it).
pub const STEP_WINDOW_CLOSE: egui_pop_model::ids::StepId =
    egui_pop_model::ids::StepId::from_static("window.close");

/// Declares the window requests' steps.
pub(crate) fn declare(ctx: &mut crate::Ctx<'_>) -> Result<(), crate::ctx::CtxError> {
    ctx.step(
        crate::steps::StepSpec::new(
            STEP_WINDOW_CLOSE,
            "A window closes (`window`, `kind`, `owner`, `reason`, `cause`): its listeners on the bus run in their declared order; `window.closed` follows. The main window's, with another window open and not quitting, moves that window into it instead (`window.reparent`).",
        )
        .args(json!({ "window": "settings", "kind": "app", "owner": "egui_pop.admin", "reason": "user" })),
    )?;
    shell::declare(ctx)?;
    Ok(())
}
