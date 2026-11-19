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
//! listens, under a name and at a place it declares, on the bus the
//! application holds ([`AppHandle::bus`]); nobody calls a neighbour to
//! tell it.
//!
//! The bus is a contract ([ADR 0030](https://github.com/quazardous/egui-pop/blob/main/docs/adr/0030-the-bus-is-a-contract.md),
//! the `bus-contract` crate): the application supplies it
//! (`AppBuilder::bus`), or egui-pop makes a hook kernel of its own.
//! egui-pop declares its points from [`CONTRACT`] and registers its
//! listeners on it; it never constructs a bus it is given.
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
//! 2. the owner's listener (`pop.admin.window` for the Settings app): the
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
//! `pop.window.open` and `pop.window.close` for it as its owner, in the call that
//! changed what it wants, and its listener (`pop.admin.window`) keeps the
//! app's state with the window on both points. A plugin disconnected has
//! its windows closed ([`OWNER_GONE`], reason `owner-gone`).
//!
//! The shell's windows are on the bus too: opening an application window
//! is its `pop.window.open` ([`AppHandle::request_window_open`], the
//! application as owner for [`AppHandle::open_window`]); the main
//! window's close, whatever asked (the system, its title bar's Close,
//! [`AppHandle::close_window`], [`AppHandle::quit`] with the reason
//! `quit`), is its `pop.window.close`, which closes every other window by its
//! own `pop.window.close` first; a window's content moving into the main
//! window when it closes, and back when the main window is asked to its
//! front, is a `pop.window.reparent` ([`AppHandle::request_window_reparent`]).
//! Detached panels' windows are the workbench's: detaching a panel is its
//! window's `pop.window.open`, docking it back its `pop.window.reparent` into the
//! main window, closing its window its `pop.window.close`, the workbench's
//! listener ([`WORKBENCH_WINDOW`]) changing the layout whoever asked.
//! Dialogs and file choices are on the bus too: a dialog coming up is its
//! `pop.popup.open` (`dialog:<n>`, parent: its window), a file choice started
//! its `pop.popup.open` (`file:<n>`), or `pop.window.open` of a native window for
//! the system's chooser ([`AppHandle::request_native_open`]); each ends by
//! its `pop.popup.close` or `pop.window.close`. A window closing closes its
//! popups and native windows first, each by its own request (reason
//! `parent-closed`), and their owners' listeners (`pop.dialogs.popup`,
//! `pop.files.choice`) cancel what they showed in the same call. The overlay
//! stack's overlays (the palette, menus, flyouts, popovers) and the egui
//! popups it follows (context menus, the menu bar's menus, hover cards)
//! are popups too: each opening and closing is its `pop.popup.open` or
//! `pop.popup.close`, asked in that call with how it closed (`how`), and a
//! close asked by anyone closes it in the stack ([`OVERLAY_STACK`]);
//! tooltips and a field's lists stay off, per-frame painting.
//!
//! The active window, the one the person works in, is the registry's too
//! ([`WindowActivate`], [`AppHandle::request_window_activate`]): a press
//! or a key in a window asks it (`user`), a window gaining the system's
//! focus asks it (`system`), the application asking a window to the
//! front asks it (`command`, raised by [`RAISE`], the one place egui-pop
//! sends `ViewportCommand::Focus`); `window.activated` is said when it
//! changed, and a window closing while active hands it to the window
//! worked in before it.
//!
//! The job lifecycle is the second family. Asking about a job is a
//! request ([`JobCancel`], [`JobPause`], [`JobResume`], [`JobRetry`], each
//! a [`JobAsk`], asked by [`AppHandle::request_job`]); whoever runs the
//! job does the work and answers with its `job.*` events. Those events,
//! and the job commands' `command.*` events, are delivered to the
//! listeners of [`JobEvents`] (`pop.job.event`) in journal order, once each
//! ([`AppHandle::deliver_events`]): the jobs plugin's model is one of them
//! (`pop.jobs.model`), so no job state changes but on the bus. A plugin
//! connecting or disconnecting is asked on [`PluginConnect`] and
//! [`PluginDisconnect`] (a [`PluginChange`]), the disconnection before
//! `plugin.disconnected` is said: the jobs a plugin leaves are cancelled
//! or said orphaned by a listener of its disconnection, and taken back by
//! one of its connection.
//!
//! Files dropped from another program are on it too: every window's pass
//! reads the files dropped on it once, as its `pop.files.drop` ([`FilesDrop`],
//! a [`FileDrop`] naming the window); whoever takes them listens, and
//! nobody taking them is said as `files.refused`.
//!
//! Settings are on it too: every change of a setting's value is said
//! once by the value store, its one writer, as `pop.settings.changed`
//! ([`SettingsChanged`], a [`SettingChange`]: path, scope, by), whether
//! it was made here or by another client of the host's store; the theme,
//! the pointer profile and the main window's frame are its listeners
//! ([`THEME_INSTALL`], [`POINTER_PROFILE`], [`FRAME_DECORATIONS`]), and an
//! application's part that follows a setting listens there too.
//!
//! A drag across windows is on it too ([`drag`]): a drag begins as
//! `pop.drag.start` ([`DragStart`], its kind and window); let go over a drop
//! target that takes it, in its window or another, it is `pop.drag.drop`
//! ([`DragDrop`]) and `drag.dropped`; refused, let go over nothing or
//! cancelled, `drag.refused` with the reason. Where it hovers each pass
//! stays off the bus (per-frame painting).
//!
//! The screens are on it too: a screen plugged in or
//! unplugged, as told by whatever runs the event loop
//! ([`crate::window::set_screens`]), is said as `pop.screen.added`
//! ([`ScreenAdded`]) or `pop.screen.removed` ([`ScreenRemoved`]), by its
//! name; on `pop.screen.removed` a detached panel's window kept on it docks
//! back ([`WORKBENCH_SCREENS`]) and every other window left off every
//! screen moves onto one ([`PLACE`]).
//!
//! The layout is on it too: every real change of the workbench's layout
//! is said once, when the gesture that made it ends, as `pop.layout.changed`
//! ([`LayoutChanged`], a [`LayoutChange`] naming its causes); the layout
//! is saved by its listener ([`LAYOUT_SAVE`]), an application's copy of
//! it follows after.
//!
//! A panel's state is on it too: whoever knows a panel's content is not
//! current asks `pop.panel.state` ([`PanelStateSet`], from any thread), and
//! the workbench's chrome shows it ([`PANEL_CHROME`]), never the panel.
//! What a declared panel is, its kind (whether it edits) included, is
//! `pop.panel.describe` ([`PanelDescribe`], answered by [`PANEL_DESCRIBE`]).
//! The interactive areas each window's last pass registered, what each
//! writes (display, navigate, edit) and whether an edit area is inert,
//! are `pop.ui.areas` ([`UiAreas`], answered by [`AREA_REGISTRY`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use bus_contract::{ListenOpts, Point, PointDecl, PointId, PointKind};
use bus_contract_defer::BusDefer;
use egui::{ViewportCommand, ViewportId};
use egui_pop_core::lock;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{AppHandle, WeakApp};
use crate::host::{Event, Seq};
use crate::windows::WindowId;

pub use bus_contract;
pub use bus_contract::{missing_targets, order_dump};
/// The capability contracts egui-pop reaches on the bus (bus-contract
/// 0.5 moved them out of the core).
pub use bus_contract_crossing;
pub use bus_contract_defer;
pub use bus_contract_journal;
pub use bus_contract_timings;
pub use egui_pop_hooks;

mod activate;
pub use activate::raise_main_window;
pub(crate) mod areas;
pub use areas::{AREA_POINTS, AREA_REGISTRY, UiAreas};
pub(crate) mod budget;
mod commands;
pub(crate) mod journal;
pub use commands::{ARGS_VALIDATOR, COMMAND_POINTS, CommandAsk, CommandRun};
pub(crate) use commands::{args_refusal, ask as ask_command_run, reason as command_refusal};
pub use journal::{HOST_JOURNAL, HostEvent, HostEvents};
pub mod drag;
pub use drag::{DRAG_POINTS, DragBegin, DragDelivery, DragDrop, DragStart, TARGET as DRAG_TARGET};
pub(crate) mod files;
pub use files::{BROWSER as FILES_BROWSER, FILES_POINTS, FileDrop, FilesDrop};
pub(crate) mod layout;
pub use layout::{
    LAYOUT_POINTS, LAYOUT_SAVE, LayoutChange, LayoutChanged, SETTLE as LAYOUT_SETTLE,
};
mod owners;
pub use owners::OWNER_GONE;
pub(crate) mod panel_describe;
pub use panel_describe::{PANEL_DESCRIBE, PANEL_DESCRIBE_POINTS, PanelDescribe, PanelDescription};
pub(crate) mod panel_state;
pub use panel_state::{
    PANEL_CHROME, PANEL_STATE_POINTS, PanelState, PanelStateAsk, PanelStateSet, TabMark, mark_id,
};
pub(crate) mod panels;
pub use panels::{WORKBENCH_OWNER, WORKBENCH_WINDOW};
pub(crate) mod overlays;
pub use overlays::{OVERLAY_OWNER, POPUP_PREFIX as OVERLAY_POPUP_PREFIX, STACK as OVERLAY_STACK};
mod popups;
pub(crate) mod settings;
pub use settings::{
    FRAME_DECORATIONS, POINTER_PROFILE, SETTINGS_POINTS, SettingChange, SettingsChanged,
    THEME_INSTALL,
};
pub(crate) mod screens;
pub use screens::{SCREEN_POINTS, ScreenAdded, ScreenChange, ScreenRemoved, WORKBENCH_SCREENS};
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
    /// How a popup closed, when the person or its stack closed it:
    /// `escape`, `outside`, `chosen`, `owner`, `below` (an overlay under
    /// it closed), `gone` (its widget no longer drawn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how: Option<String>,
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
            how: None,
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

    /// How the popup closed (`escape`, `chosen`…).
    pub fn how(mut self, how: impl Into<String>) -> Self {
        self.how = Some(how.into());
        self
    }
}

/// A window opens (`pop.window.open`); its listeners say what they did, one
/// line each, in the value.
#[derive(Debug)]
pub struct WindowOpen;
impl Point for WindowOpen {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.window.open");
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A window closes (`pop.window.close`).
#[derive(Debug)]
pub struct WindowClose;
impl Point for WindowClose {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.window.close");
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A window's content moves to another window (`pop.window.reparent`).
#[derive(Debug)]
pub struct WindowReparent;
impl Point for WindowReparent {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.window.reparent");
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A popup opens in a window (`pop.popup.open`).
#[derive(Debug)]
pub struct PopupOpen;
impl Point for PopupOpen {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.popup.open");
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A popup closes (`pop.popup.close`).
#[derive(Debug)]
pub struct PopupClose;
impl Point for PopupClose {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.popup.close");
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// A window becomes the active one, the window worked in (`pop.window.activate`):
/// the person pressed or typed in it (`user`), the system gave it the
/// focus (`system`), or the application asks it to the front (`command`,
/// raised by [`RAISE`]).
#[derive(Debug)]
pub struct WindowActivate;
impl Point for WindowActivate {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.window.activate");
    type Input = WindowRequest;
    type Value = Vec<String>;
}

/// The window and popup points, in the order [`order_dump`] prints them.
pub const WINDOW_POINTS: [&str; 6] = [
    WindowOpen::ID.as_str(),
    WindowClose::ID.as_str(),
    WindowReparent::ID.as_str(),
    WindowActivate::ID.as_str(),
    PopupOpen::ID.as_str(),
    PopupClose::ID.as_str(),
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

/// A job is asked to stop (`pop.job.cancel`); its listeners say what they
/// did, one line each, in the value.
#[derive(Debug)]
pub struct JobCancel;
impl Point for JobCancel {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.job.cancel");
    type Input = JobAsk;
    type Value = Vec<String>;
}

/// A job is asked to pause (`pop.job.pause`).
#[derive(Debug)]
pub struct JobPause;
impl Point for JobPause {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.job.pause");
    type Input = JobAsk;
    type Value = Vec<String>;
}

/// A paused job is asked to go on (`pop.job.resume`).
#[derive(Debug)]
pub struct JobResume;
impl Point for JobResume {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.job.resume");
    type Input = JobAsk;
    type Value = Vec<String>;
}

/// A failed or cancelled job is asked to run again (`pop.job.retry`).
#[derive(Debug)]
pub struct JobRetry;
impl Point for JobRetry {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.job.retry");
    type Input = JobAsk;
    type Value = Vec<String>;
}

/// A job event was said (`pop.job.event`): each `job.*` event of the journal, and
/// each `command.*` event of a job command, delivered once, in journal
/// order ([`AppHandle::deliver_events`]).
#[derive(Debug)]
pub struct JobEvents;
impl Point for JobEvents {
    const KIND: PointKind = PointKind::Event;
    const ID: PointId = PointId::new("pop.job.event");
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

/// A plugin is connected (`pop.plugin.connect`), what it registers in
/// place; its listeners take back what it had left.
#[derive(Debug)]
pub struct PluginConnect;
impl Point for PluginConnect {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.plugin.connect");
    type Input = PluginChange;
    type Value = Vec<String>;
}

/// A plugin is disconnected (`pop.plugin.disconnect`), what it registered
/// removed, before `plugin.disconnected` is said: each listener adds what
/// it did to the value (`jobs_cancelled`…), which the event's `removed`
/// carries.
#[derive(Debug)]
pub struct PluginDisconnect;
impl Point for PluginDisconnect {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.plugin.disconnect");
    type Input = PluginChange;
    type Value = serde_json::Map<String, serde_json::Value>;
}

/// The job points, in the order [`order_dump`] prints them.
pub const JOB_POINTS: [&str; 5] = [
    JobCancel::ID.as_str(),
    JobPause::ID.as_str(),
    JobResume::ID.as_str(),
    JobRetry::ID.as_str(),
    JobEvents::ID.as_str(),
];

/// The plugin points, in the order [`order_dump`] prints them.
pub const PLUGIN_POINTS: [&str; 2] = [PluginConnect::ID.as_str(), PluginDisconnect::ID.as_str()];

/// Every point egui-pop declares: the window, job, plugin, files,
/// settings, drag, screen, panel state, layout, panel description and
/// command and area points.
pub const POINTS: [&str; 24] = [
    WindowOpen::ID.as_str(),
    WindowClose::ID.as_str(),
    WindowReparent::ID.as_str(),
    WindowActivate::ID.as_str(),
    PopupOpen::ID.as_str(),
    PopupClose::ID.as_str(),
    JobCancel::ID.as_str(),
    JobPause::ID.as_str(),
    JobResume::ID.as_str(),
    JobRetry::ID.as_str(),
    JobEvents::ID.as_str(),
    PluginConnect::ID.as_str(),
    PluginDisconnect::ID.as_str(),
    FilesDrop::ID.as_str(),
    SettingsChanged::ID.as_str(),
    DragStart::ID.as_str(),
    DragDrop::ID.as_str(),
    ScreenAdded::ID.as_str(),
    ScreenRemoved::ID.as_str(),
    PanelStateSet::ID.as_str(),
    LayoutChanged::ID.as_str(),
    PanelDescribe::ID.as_str(),
    CommandRun::ID.as_str(),
    UiAreas::ID.as_str(),
];

/// egui-pop's contract table (ADR 0030): every point it declares on the
/// bus it is given, with its kind and description, readable before an
/// application is built.
pub const CONTRACT: &[PointDecl] = &[
    PointDecl::of::<WindowOpen>("A window opens: its id, kind, owner, parent."),
    PointDecl::of::<WindowClose>(
        "A window closes, whoever asked: its children, its owner's state, the focus, the registry and its place follow, in one call.",
    ),
    PointDecl::of::<WindowReparent>(
        "A window's content moves to another window (into the main window and back).",
    ),
    PointDecl::of::<WindowActivate>(
        "A window becomes the active one (the person worked in it, the system focused it, or a command asks it to the front): the registry holds it as active, a command's raises it; `window.activated` follows when it changed.",
    ),
    PointDecl::of::<PopupOpen>("A popup opens in a window."),
    PointDecl::of::<PopupClose>("A popup closes."),
    PointDecl::of::<JobCancel>(
        "A job is asked to stop: whoever runs it is asked, and the job shows as asked until it answers.",
    ),
    PointDecl::of::<JobPause>("A job is asked to pause."),
    PointDecl::of::<JobResume>("A paused job is asked to go on."),
    PointDecl::of::<JobRetry>("A failed or cancelled job is asked to run again."),
    PointDecl::of::<JobEvents>(
        "A `job.*` event, or a job command's `command.*` event, was said: delivered once each, in journal order.",
    ),
    PointDecl::of::<PluginConnect>("A plugin is connected: what it had left is taken back."),
    PointDecl::of::<PluginDisconnect>(
        "A plugin is disconnected, before `plugin.disconnected` is said: what it leaves is closed or marked, each listener counting what it did.",
    ),
    PointDecl::of::<FilesDrop>(
        "Files from another program are dropped on a window: whoever takes them says so; `files.dropped` follows, or `files.refused` when nobody took them.",
    ),
    PointDecl::of::<SettingsChanged>(
        "A setting's value changed, here or by another client of the host's store: said once by the value store, its one writer (path, scope, by); whoever follows the setting does its part.",
    ),
    PointDecl::of::<DragStart>(
        "A drag begins in a window (a tab, a tab group, an application's payload): its kind and the window; `drag.started` follows.",
    ),
    PointDecl::of::<DragDrop>(
        "A drag is let go over a drop target that takes it, in its window or another: the window and the target; `drag.dropped` follows. A target refusing it, a drag let go over nothing, or cancelled, is said as `drag.refused` with the reason.",
    ),
    PointDecl::of::<ScreenAdded>(
        "A screen was plugged in, as told by whatever runs the event loop: its name as the system gives it, its rectangle in points, its scale factor.",
    ),
    PointDecl::of::<ScreenRemoved>(
        "A screen was unplugged: a detached panel's window kept on it docks back, every other window left off every screen moves onto one (not where the system hides window positions).",
    ),
    PointDecl::of::<PanelStateSet>(
        "A panel's state is asked, from any thread: its content stale, updating, failed (with the reason) or ready again; the panel's chrome shows it (content dimmed, a delayed busy mark on its corner and tab), never the panel.",
    ),
    PointDecl::of::<LayoutChanged>(
        "The workbench's layout changed (a panel docked, detached, floated, moved, opened or closed, a split or a palette resized, a layout put in): said once per change, once its gesture ended; the layout is saved to the host's document by `layout.save`, an application's copy follows it.",
    ),
    PointDecl::of::<PanelDescribe>(
        "A declared panel is described, by its id: its title, whether it is a palette, and its kind (view, control or mixed: whether it edits), the kind whose mark its chrome shows; asked on the interface's thread.",
    ),
    PointDecl::of::<CommandRun>(
        "A command is about to run, whatever asked it (a key, the palette, a menu, a toolbar, an agent): a listener may refuse it with a reason, and the handler does not run; fail-closed, so a listener that fails refuses it too. `command.refused` names who refused and why.",
    )
    .fail_closed(),
    PointDecl::of::<UiAreas>(
        "The interactive areas are listed, each window's last pass: each one's kind (display, navigate or edit), for an edit area the control it writes, its value and path, its panel, its grips, and whether it is inert (its panel's state not current); asked on the interface's thread.",
    ),
];

/// What owns egui-pop's own listeners.
pub const OWNER: &str = "egui_pop";
/// What belongs to a closing window goes with it: its dialogs.
pub const CHILDREN: &str = "pop.window.children";
/// The window worked in before a closing one becomes the active one.
pub const FOCUS: &str = "pop.window.focus";
/// A window asked to the front by a command (`pop.window.activate`, reason
/// `command`) is shown, restored and given the system's focus: the one
/// place egui-pop sends `ViewportCommand::Focus`.
pub const RAISE: &str = "pop.window.raise";
/// Another window becoming the active one drops every window's shortcut
/// in progress: a chord begun in one window is never finished in another
/// (a local listener: the windows' key dispatchers are the interface's).
pub const KEYS: &str = "pop.window.keys";
/// The one list of windows and popups ([`AppHandle::windows_list`]).
pub const REGISTRY: &str = "pop.window.registry";
/// A window's place, kept when it closes, and moved onto a screen left
/// when its screen is unplugged (`pop.screen.removed`).
pub const PLACE: &str = "pop.window.place";
/// Checks, last, the window invariants ([`crate::window_invariants`]).
pub const INVARIANTS: &str = "pop.window.invariants";

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
    /// Whether it is the active window, the one worked in
    /// (`pop.window.activate`): one window of the list is.
    pub active: bool,
    /// What draws into its own surface besides egui, as that part says it
    /// (`egui_pop_core::window_surface`): the image viewport's image
    /// surface, its format, colour space, route or why it has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<serde_json::Value>,
    /// Whether its native window's handles are known (`runner::window`
    /// answers for its viewport): what a surface of its own needs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub handles: bool,
    /// How its native window was built for a surface below egui:
    /// transparent, or opaque and why
    /// (`egui_pop_core::window_surface::transparency`), when its builder
    /// said it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transparency: Option<egui_pop_core::window_surface::Transparency>,
    /// Its viewport, to read `surface`.
    #[serde(skip)]
    pub(crate) viewport_id: Option<ViewportId>,
}

impl WindowRecord {
    /// Its viewport, when it has one.
    pub fn viewport_id(&self) -> Option<ViewportId> {
        self.viewport_id
    }
}

/// The registry's state: the windows and popups open, the main window
/// first, then in opening order.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    records: Vec<WindowRecord>,
    /// The windows activated (`pop.window.activate`), the active one last.
    activated: Vec<String>,
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
            active: false,
            surface: None,
            handles: false,
            transparency: None,
            viewport_id: viewport,
        });
    }

    fn close(&mut self, id: &str) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.id != id);
        self.activated.retain(|a| a != id);
        before != self.records.len()
    }

    fn record(&self, id: &str) -> Option<&WindowRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    /// `id` is the active window.
    fn activate(&mut self, id: &str) {
        self.activated.retain(|a| a != id);
        self.activated.push(id.to_string());
    }

    /// The windows activated still held, the active one first.
    fn by_recency(&self) -> impl Iterator<Item = &WindowRecord> {
        self.activated.iter().rev().filter_map(|a| self.record(a))
    }

    /// The active window: the last activated still held, else the main
    /// window.
    pub(crate) fn active(&self) -> (String, WindowKind) {
        self.by_recency()
            .next()
            .map(|r| (r.id.clone(), r.kind))
            .unwrap_or_else(|| (WindowId::MAIN.to_string(), WindowKind::Main))
    }

    fn holds(&self, id: &str) -> bool {
        self.records.iter().any(|r| r.id == id)
    }

    /// The children of `from` (its popups, its native windows) become
    /// `to`'s: `from`'s content moved there. How many.
    fn adopt(&mut self, from: &str, to: &str) -> usize {
        let mut moved = 0;
        for r in &mut self.records {
            if r.parent.as_deref() == Some(from) && r.id != to {
                r.parent = Some(to.to_string());
                moved += 1;
            }
        }
        moved
    }
}

/// The bus an application holds: the bus it was given (or the default
/// kernel) and the window registry.
pub(crate) struct Bus {
    pub(crate) kernel: Arc<dyn bus_contract::Bus>,
    pub(crate) registry: Arc<Mutex<Registry>>,
    /// What the window invariants remember between passes
    /// ([`crate::window_invariants`]).
    pub(crate) invariants: Mutex<crate::window_invariants::State>,
    /// The screens this application follows ([`screens`]).
    pub(crate) screens: RefCell<screens::Seen>,
    /// Whether each window had the system's focus at its last pass: a
    /// window gaining it asks `pop.window.activate` (`system`) once.
    focus_seen: RefCell<std::collections::BTreeMap<ViewportId, bool>>,
    /// Application windows raised by a command, asked the focus again
    /// from their own next pass (one not drawn yet has no system window).
    raise_pending: RefCell<std::collections::BTreeSet<ViewportId>>,
    /// Counts the changes of the active window ([`KEYS`]): a window's key
    /// dispatcher that saw another count drops its shortcut in progress.
    pub(crate) keys_epoch: Rc<Cell<u64>>,
    /// The last journal event delivered to the event points' listeners.
    delivered: Cell<Seq>,
    /// A delivery runs: one asked from inside it is left to it.
    delivering: Cell<bool>,
    /// The settings the value store changed, not said yet
    /// ([`AppHandle::deliver_settings`]).
    pub(crate) settings_heard: settings::Heard,
    /// A delivery of settings runs.
    settings_delivering: Cell<bool>,
    /// The context a pass is asked of when an emit on another thread
    /// queues local listeners ([`AppHandle::run_pending`]).
    wake: Arc<Mutex<Option<egui::Context>>>,
    /// The overlays of the stack the registry holds as popups.
    pub(crate) overlays: RefCell<overlays::Overlays>,
    /// Each panel's state not ready (`pop.panel.state`), for its chrome.
    pub(crate) panel_states: panel_state::States,
    /// Passes over a point's budget, to say (`bus.overBudget`).
    pub(crate) over_budget: budget::Heard,
}

impl std::fmt::Debug for Bus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bus").finish_non_exhaustive()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Bus::new(None)
    }
}

impl Bus {
    /// On the bus the application supplies (`AppBuilder::bus`), or on
    /// the default kernel: egui-pop's points declared, its listeners
    /// registered.
    pub(crate) fn new(supplied: Option<Arc<dyn bus_contract::Bus>>) -> Self {
        let kernel = supplied.unwrap_or_else(|| Arc::new(egui_pop_hooks::Kernel::new()));
        let mut main = Registry::default();
        main.open(
            &WindowRequest::new(WindowId::MAIN, WindowKind::Main, "app", Reason::System),
            Some(ViewportId::ROOT),
        );
        main.activate(WindowId::MAIN);
        let registry = Arc::new(Mutex::new(main));
        install(&*kernel, &registry);
        let keys_epoch = Rc::new(Cell::new(0));
        activate::install_keys(&*kernel, keys_epoch.clone());
        let wake: Arc<Mutex<Option<egui::Context>>> = Arc::default();
        {
            let wake = wake.clone();
            // Local listeners queued and requests asked for after the
            // drawing both wait for the main window's pass, whichever
            // window asked: it runs them (`run_pending`, `drain_asks`).
            kernel.on_pending(move |_| {
                if let Some(ctx) = lock(&wake).as_ref() {
                    ctx.request_repaint_of(ViewportId::ROOT);
                }
            });
        }
        let panel_states = panel_state::States::default();
        panel_state::install(&*kernel, &panel_states, wake.clone());
        panel_describe::install(&*kernel);
        areas::install(&*kernel, wake.clone());
        let over_budget = budget::install(&*kernel);
        Bus {
            kernel,
            registry,
            invariants: Mutex::default(),
            screens: RefCell::default(),
            focus_seen: RefCell::default(),
            raise_pending: RefCell::default(),
            keys_epoch,
            delivered: Cell::new(0),
            delivering: Cell::new(false),
            settings_heard: settings::Heard::default(),
            settings_delivering: Cell::new(false),
            wake,
            overlays: RefCell::default(),
            panel_states,
            over_budget,
        }
    }

    /// The context to wake when local listeners are queued for the
    /// interface's thread.
    pub(crate) fn wake_with(&self, ctx: &egui::Context) {
        let mut wake = lock(&self.wake);
        if wake.as_ref() != Some(ctx) {
            *wake = Some(ctx.clone());
        }
    }
}

thread_local! {
    /// The applications whose request is running, innermost last: what a
    /// listener acts on. Listeners run inline, on the thread that asked.
    static ASKING: RefCell<Vec<WeakApp>> = const { RefCell::new(Vec::new()) };
}

/// The application whose request runs now, for a listener (listeners
/// are `Send + Sync` and keep no handle): an application's listener of
/// `pop.files.drop` runs its import command on it. `None` outside a request.
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn asking() -> Option<AppHandle> {
    ASKING.with_borrow(|a| a.last().and_then(|w| w.get().ok()))
}

thread_local! {
    /// The context whose main window's pass runs [`AppHandle::run_pending`]
    /// now: a listener it runs is drawn by that pass, which asks for no
    /// other main pass ([`AppHandle::wake_window`]).
    static MAIN_PENDING: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

/// Marks [`MAIN_PENDING`] while the main window's pass runs the pending
/// listeners; the previous mark back when they end, even by a panic.
struct MainPending(Option<egui::Context>);

impl MainPending {
    fn enter(ctx: egui::Context) -> Self {
        Self(MAIN_PENDING.with_borrow_mut(|m| m.replace(ctx)))
    }
}

impl Drop for MainPending {
    fn drop(&mut self) {
        let previous = self.0.take();
        MAIN_PENDING.with_borrow_mut(|m| *m = previous);
    }
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
fn install(kernel: &dyn bus_contract::Bus, registry: &Arc<Mutex<Registry>>) {
    // A second application on the same bus finds its points declared
    // (ADR 0030: one egui-pop application per bus).
    for refused in bus_contract::declare_all(kernel, CONTRACT) {
        log::error!(target: "egui_pop", "{refused}: one egui-pop application per bus");
    }
    let mine = || ListenOpts::new().owner(OWNER);
    // `pop.window.close`, in the ADR's order.
    let listened = [
        kernel.listen::<WindowClose>(CHILDREN, mine().before(FOCUS).priority(1000), |req, did| {
            let Some(app) = asking() else { return Ok(()) };
            if req.kind == WindowKind::Main {
                shell::close_children_of_main(&app, req, did);
            }
            popups::close_children(&app, req, did);
            Ok(())
        }),
        kernel.listen::<WindowClose>(FOCUS, mine().before(REGISTRY), |req, did| {
            let Some(app) = asking() else { return Ok(()) };
            app.forget_focus(req);
            if app.focus_before_close(&req.window).is_some() {
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
                        WindowKind::Panel if held => panels::left(&app, &req.window),
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
    activate::install(kernel, registry);
    panels::install(kernel);
    owners::install(kernel);
    files::install(kernel);
    overlays::install(kernel);
    drag::install(kernel);
    screens::install(kernel);
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
        kernel.listen::<WindowActivate>(INVARIANTS, mine().last(), check),
        kernel.listen::<PopupOpen>(INVARIANTS, mine().last(), check),
        kernel.listen::<PopupClose>(INVARIANTS, mine().last(), check),
    ];
    for l in others {
        debug_assert!(l.is_ok(), "{l:?}");
    }
}

impl AppHandle {
    /// Registers a local listener on `P` under `id` for the window
    /// `window`: each time it runs, that window is woken, so what it
    /// changed is drawn there within one frame. Bound to the calling
    /// thread (the interface's), as [`bus_contract::Bus::listen_local_any`].
    ///
    /// An emit on this thread runs it inline and asks for a pass of the
    /// window. An emit from another thread queues it: the bus waker asks
    /// for the main window's pass, which runs it ([`AppHandle::run_pending`];
    /// a request asked for after the drawing is heard at the main
    /// window's drain), and its run then asks for the window's pass.
    /// The main window is woken as before; the window is woken as well,
    /// rather than redrawn by hand. A window drawn in the main one (the
    /// main window itself, a detached panel's, every window where the
    /// windows are embedded: the web, kittest) and a window not open
    /// wake the main window.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn listen_local_in<P: Point>(
        &self,
        window: &WindowId,
        id: impl Into<String>,
        opts: ListenOpts,
        run: impl Fn(&P::Input, &mut P::Value) -> Result<(), bus_contract::HookError> + 'static,
    ) -> Result<(), bus_contract::Refused> {
        let (app, window) = (self.downgrade(), window.clone());
        self.bus().listen_local::<P>(id, opts, move |input, value| {
            let out = run(input, value);
            if let Ok(app) = app.get() {
                app.wake_window(&window);
            }
            out
        })
    }

    /// Asks for a pass of the window `window`: its own viewport's when
    /// it is an open application window with a system window of its own;
    /// else the main window's, where it is drawn (or nothing is).
    /// Asked by a listener [`AppHandle::run_pending`] runs in the main
    /// window's pass, the main window's pass is not asked for: that pass
    /// has not drawn yet, so it draws what the listener changed.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn wake_window(&self, window: &WindowId) {
        let ctx = self.egui();
        let wake_main = || {
            if !MAIN_PENDING.with_borrow(|m| m.as_ref() == Some(&ctx)) {
                ctx.request_repaint_of(ViewportId::ROOT);
            }
        };
        let viewport = match self.0.windows.try_borrow() {
            Ok(w) => w.viewport_of(window),
            // Asked while the registry is changed (a listener run from a
            // window's opening or closing): its viewport and the main
            // window's, whichever draws it.
            Err(_) => {
                wake_main();
                Some(window.deferred_viewport())
            }
        };
        match viewport {
            Some(v) if v != ViewportId::ROOT && !ctx.embed_viewports() => {
                ctx.request_repaint_of(v);
            }
            _ => wake_main(),
        }
    }

    /// Runs the local listeners ([`bus_contract::Bus::listen_local_any`])
    /// that emits on other threads queued for this thread, in their
    /// order, with this application as [`asking`]; their failures
    /// logged. The application calls it once per pass, before the
    /// `On::Pass` handlers; an emit that queues some asks for a pass.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn run_pending(&self) -> Vec<bus_contract::PendingRun> {
        ASKING.with_borrow_mut(|a| a.push(self.downgrade()));
        let _asking = Asking;
        let ctx = self.egui();
        let in_main = matches!(
            crate::app::pass_in_progress(&ctx),
            Some((ViewportId::ROOT, _))
        );
        let _main = in_main.then(|| MainPending::enter(ctx));
        let ran = self.0.bus.kernel.run_pending();
        for f in ran.iter().flat_map(|r| &r.failures) {
            log::error!(target: "egui_pop", "{f}");
        }
        ran
    }

    /// Asks the request `P` now and hears it after the drawing: its
    /// listeners run at the bus's next drain on this thread
    /// ([`AppHandle::drain_asks`]), which the application makes once per
    /// pass after the panels and windows are drawn, and once more before
    /// it quits. For code that cannot be re-entered while it draws (a
    /// panel with its state borrowed). The ask's number; `None` when the
    /// application's bus does not offer the `Defer` capability
    /// ([`bus_contract_defer::BusDefer::defer`]), the caller then asking it at once
    /// or keeping it.
    ///
    /// Asked in a detached panel's window, drawn inside the main window's
    /// pass, it is heard at the end of that pass. Asked in an application
    /// window (a deferred viewport, drawn on the same thread after the
    /// main window), it waits in the same queue for the main window's
    /// next pass, which is asked for.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn ask<P: Point>(&self, input: P::Input, value: P::Value) -> Option<u64>
    where
        P::Value: Clone,
    {
        let bus = self.bus();
        let defer = bus.defer()?;
        Some(defer.ask::<P>(input, value))
    }

    /// Runs the requests asked on this thread for after the drawing
    /// ([`AppHandle::ask`], or [`bus_contract_defer::Defer`] on the
    /// application's bus), in ask order, with this application as
    /// [`asking`]; what each did, failures logged and refusals noted.
    /// An ask made by a listener while it runs waits for the next drain.
    /// A drain that ran something asks for a pass, so what it changed is
    /// drawn. Empty when nothing waits or the bus has no `Defer`.
    ///
    /// The application calls it once per pass after the panels and
    /// windows are drawn ([`crate::App::show`]) and before it quits
    /// ([`crate::App::exit`]); a host drawing the application in its own
    /// way may call it too.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn drain_asks(&self) -> Vec<bus_contract::PendingRun> {
        let bus = self.bus();
        let Some(defer) = bus.defer() else {
            return Vec::new();
        };
        if defer.waiting() == 0 {
            return Vec::new();
        }
        ASKING.with_borrow_mut(|a| a.push(self.downgrade()));
        let _asking = Asking;
        let ran = defer.drain();
        for r in &ran {
            for f in &r.failures {
                log::error!(target: "egui_pop", "{f}");
            }
            if let Some(refused) = &r.refused {
                log::info!(target: "egui_pop", "{refused}");
            }
        }
        if !ran.is_empty() {
            self.egui().request_repaint_of(ViewportId::ROOT);
        }
        ran
    }

    /// How many requests wait for this thread's next drain
    /// ([`AppHandle::drain_asks`]): zero when the bus has no `Defer`.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn asks_waiting(&self) -> usize {
        self.bus().defer().map_or(0, |d| d.waiting())
    }

    /// The application's bus: the one given to `AppBuilder::bus`, or the
    /// hook kernel egui-pop made, where its requests' listeners are
    /// registered (ADR 0029, ADR 0030). A part that cares about a window closing
    /// listens to [`WindowClose`] under a name of its own, declaring its
    /// place (`after` / `before` the listeners of this module).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn bus(&self) -> Arc<dyn bus_contract::Bus> {
        self.0.bus.kernel.clone()
    }

    /// The windows and popups on screen, as the registry holds them: the
    /// main window first, then in opening order (`windows.list`).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn windows_list(&self) -> Vec<WindowRecord> {
        let r = lock(&self.0.bus.registry);
        let (active, _) = r.active();
        let mut list = r.records.clone();
        drop(r);
        let ctx = self.egui();
        for w in &mut list {
            w.active = w.id == active;
            w.surface = w
                .viewport_id
                .and_then(|v| egui_pop_core::window_surface::get(&ctx, v));
            w.transparency = w
                .viewport_id
                .and_then(|v| egui_pop_core::window_surface::transparency(&ctx, v));
            #[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
            {
                w.handles = w
                    .viewport_id
                    .is_some_and(|v| crate::runner::window(v).is_some());
            }
        }
        list
    }

    /// The order the listeners of the window and popup points run in
    /// ([`order_dump`]).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn bus_order(&self) -> String {
        order_dump(&*self.0.bus.kernel, &WINDOW_POINTS)
    }

    /// Asks for an application window's close (`pop.window.close`): its
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
            _ => lock(&self.0.bus.registry).holds(&request.window),
        };
        if !open {
            self.publish(
                "egui_pop",
                None,
                "window.refused",
                json!({ "request": WindowClose::ID.as_str(), "window": request.window, "owner": request.owner, "reason": "no open window of this id" }),
            );
            return false;
        }
        let args = serde_json::to_value(&request).unwrap_or_default();
        let outcome = self.step(&STEP_WINDOW_CLOSE, args, |app, args| {
            // A before-hook may have redirected it.
            let req = serde_json::from_value::<WindowRequest>(args.clone())
                .map_err(|e| format!("not a window request: {e}"))?;
            // The main window's close with another window open moves
            // that window into it (ADR 0025): a `pop.window.reparent`.
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
                    json!({ "request": WindowClose::ID.as_str(), "window": request.window, "owner": request.owner, "vetoed_by": by, "reason": reason }),
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
        order_dump(&*self.0.bus.kernel, points)
    }

    /// Asks a panel's state (`pop.panel.state`): its listeners run now, in
    /// their declared order (the workbench's keeps it for the panel's
    /// chrome). What they did, one line each. An application holding the
    /// bus may ask it there, from any thread.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_panel_state(&self, ask: PanelStateAsk) -> Vec<String> {
        emit_with::<PanelStateSet>(self, ask, Vec::new())
    }

    /// Describes a declared panel (`pop.panel.describe`): its listeners run
    /// now (the workbench's fills it from the declaration, its kind
    /// included); `None` for a panel nobody knows.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn describe_panel(&self, panel: &str) -> Option<PanelDescription> {
        emit_with::<PanelDescribe>(self, panel.to_string(), None)
    }

    /// The interactive areas of each window's last pass (`pop.ui.areas`):
    /// what each writes, an edit area's control, value and grips, and
    /// whether it is inert.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn ui_areas(&self) -> Vec<egui_pop_core::areas::AreaRecord> {
        emit_with::<UiAreas>(self, (), Vec::new())
    }

    /// A panel's state as last asked (`pop.panel.state`): ready when never
    /// asked.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn panel_state(&self, panel: &str) -> PanelState {
        lock(&self.0.bus.panel_states)
            .get(panel)
            .map_or(PanelState::Ready, |k| k.state.clone())
    }

    /// Asks about a job (`pop.job.cancel`, `pop.job.pause`, `pop.job.resume`,
    /// `pop.job.retry`): its listeners run now, in their declared order (the
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

    /// `pop.window.registry`'s part on an application window: marked closed,
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
            "A window closes (`window`, `kind`, `owner`, `reason`, `cause`): its listeners on the bus run in their declared order; `window.closed` follows. The main window's, with another window open and not quitting, moves that window into it instead (`pop.window.reparent`).",
        )
        .args(json!({ "window": "settings", "kind": "app", "owner": "egui_pop.admin", "reason": "user" })),
    )?;
    shell::declare(ctx)?;
    Ok(())
}
