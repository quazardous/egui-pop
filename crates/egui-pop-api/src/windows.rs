//! Application windows: several OS windows in one process, each almost an
//! application of its own (ADR 0025).
//!
//! The application opens a window with [`AppHandle::open_window`], giving
//! a [`WindowSpec`] and the window's content: a closure drawing it each
//! pass, which may hold `Rc` state. Each window is an egui **deferred
//! viewport**: it repaints on its own, so an idle window sleeps while
//! another animates. The windows are declared from the main window's pass
//! ([`App::show`](crate::App::show)); the main window is the root, always
//! the first window, and its content is the application's shell.
//!
//! - **The `Rc` shim.** egui asks a deferred window's callback to be
//!   `Send + Sync`. eframe runs it on the main thread, so the callback
//!   holds only the window's viewport id and finds the window and the
//!   application in a thread-local registry; it asserts (in debug builds)
//!   that it runs on the thread that opened the window. Off that thread
//!   the registry is empty and the window draws nothing.
//! - **Titles come from the content.** [`WindowUi::set_title`] names the
//!   window after what it does ("Import — card 3"), and a change reaches
//!   the system's title bar at once.
//! - **Closing.** A window's close button closes it, unless its content
//!   calls [`WindowUi::keep_open`] in the pass that sees the request
//!   ([`WindowUi::close_requested`]). Closing the main window while other
//!   windows are open does not quit: the close is cancelled and the window
//!   worked in last moves into the main window (its content, title and
//!   id), its own window closing. Closing the last window quits.
//! - **Geometry** is read from each window's own pass (egui's
//!   `ViewportInfo` rects stay empty for deferred windows on Wayland) and
//!   saved per window id in the persistence directory (`windows.json`),
//!   then given back when a window of that id opens again. The position
//!   only where the platform tells it (never on Wayland).
//! - **Per window:** its keys are dispatched in its own pass, with the
//!   `appWindow` context key naming it (`when: "appWindow == 'import'"`);
//!   dialogs, the palette and popup menus open over the window worked in
//!   last; a modal open in one application window blocks that window
//!   only (the main window and its detached panels block one another,
//!   as before).
//! - **Present mode.** On Wayland, a FIFO present waits on the compositor
//!   in the one UI thread and can stall every window (seen on headless
//!   mutter). [`AppBuilder::multi_window`](crate::AppBuilder::multi_window)
//!   makes `run_native` use wgpu's `AutoNoVsync` there (paced by winit's
//!   frame callbacks), keeping `AutoVsync` elsewhere; [`present_mode`]
//!   does it for an application running eframe itself.
//!
//! Where the backend has no windows of its own (the web, kittest), egui
//! embeds each window in the main one as an `egui::Window`: the same
//! content runs there.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::thread::ThreadId;

use egui::{Context, Ui, Vec2, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};
use egui_pop_core::keys::KeyDispatcher;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{AppHandle, WeakApp};

/// The context key naming the application window being drawn (its
/// [`WindowId`]; `main` in the main window until another moves into it).
pub const CONTEXT_APP_WINDOW: &str = "appWindow";

/// Where the windows' geometry is kept, under the persistence directory.
pub const GEOMETRY_DOC: &str = "windows";

/// An application window's id: stable across runs, as its geometry is
/// saved under it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowId(String);

impl WindowId {
    /// The main window's id at start.
    pub const MAIN: &'static str = "main";

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The main window's id at start.
    pub fn main() -> Self {
        Self(Self::MAIN.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The viewport a window of this id gets when it is not the main one.
    fn deferred_viewport(&self) -> ViewportId {
        ViewportId::from_hash_of(("egui_pop.app_window", self.0.as_str()))
    }
}

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for WindowId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// What a window opens with.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowSpec {
    pub id: WindowId,
    /// The first title, until the content sets its own.
    pub title: String,
    /// The inner size at its first opening (a saved size wins).
    pub size: Option<Vec2>,
    pub min_size: Option<Vec2>,
}

impl WindowSpec {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: WindowId::new(id),
            title: title.into(),
            size: None,
            min_size: None,
        }
    }

    pub fn size(mut self, size: impl Into<Vec2>) -> Self {
        self.size = Some(size.into());
        self
    }

    pub fn min_size(mut self, size: impl Into<Vec2>) -> Self {
        self.min_size = Some(size.into());
        self
    }
}

/// A window's place, as saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    /// The inner size, in points.
    pub size: [f32; 2],
    /// The outer position, when the platform tells it (not on Wayland).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 2]>,
    #[serde(default)]
    pub maximized: bool,
}

/// An open window, as [`AppHandle::windows`] lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WindowInfo {
    pub id: WindowId,
    pub title: String,
    /// The main window (the root viewport).
    pub main: bool,
    /// The window the user worked in last.
    pub active: bool,
    /// Whether the system says it has the keyboard focus (unknown until
    /// it drew once).
    pub focused: Option<bool>,
    /// Its geometry, as last seen.
    pub geometry: Option<Geometry>,
    #[serde(skip)]
    pub viewport: ViewportId,
}

/// The window being drawn, for its content.
pub struct WindowUi<'a> {
    app: &'a AppHandle,
    id: WindowId,
    title: String,
    class: ViewportClass,
    close_requested: bool,
    keep_open: bool,
    close: bool,
}

impl fmt::Debug for WindowUi<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowUi")
            .field("id", &self.id)
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl<'a> WindowUi<'a> {
    pub fn id(&self) -> &WindowId {
        &self.id
    }

    pub fn app(&self) -> &'a AppHandle {
        self.app
    }

    /// The window's title now.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Names the window after what it shows; the system's title bar
    /// follows at once.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// The user asked to close the window this pass (its close button,
    /// the system's shortcut).
    pub fn close_requested(&self) -> bool {
        self.close_requested
    }

    /// Keeps the window open after a close request (to ask a question
    /// first, then [`WindowUi::close`]).
    pub fn keep_open(&mut self) {
        self.keep_open = true;
    }

    /// Closes the window at the end of this pass.
    pub fn close(&mut self) {
        self.close = true;
    }

    /// Whether the window is drawn inside the main one (a backend without
    /// windows of its own: the web, kittest).
    pub fn embedded(&self) -> bool {
        self.class == ViewportClass::EmbeddedWindow
    }
}

/// A window's content: drawn each pass of its window.
pub type Content = Box<dyn FnMut(&mut Ui, &mut WindowUi<'_>)>;

pub(crate) struct Entry {
    pub(crate) id: WindowId,
    pub(crate) title: String,
    /// The title the system was last given.
    sent_title: Option<String>,
    /// Out of its cell while it draws.
    content: Option<Content>,
    /// The builder's first geometry, kept as is: a builder changed later
    /// would move the window.
    first: (Option<Vec2>, Option<[f32; 2]>, bool),
    min_size: Option<Vec2>,
    pub(crate) geometry: Option<Geometry>,
    pub(crate) focused: Option<bool>,
    /// When it was last worked in (a tick, larger is later).
    pub(crate) last_active: u64,
    dispatcher: Option<KeyDispatcher>,
    raise: bool,
    pub(crate) closed: bool,
}

/// The main window's slot: the application's shell, or the content of a
/// window moved into it.
pub(crate) struct Root {
    pub(crate) id: WindowId,
    /// Set by the application or by the content moved in.
    pub(crate) title: Option<String>,
    sent_title: Option<String>,
    content: Option<Content>,
    pub(crate) last_active: u64,
    pub(crate) geometry: Option<Geometry>,
}

/// The application's windows.
pub(crate) struct Windows {
    pub(crate) root: Root,
    /// The windows other than the main one, in opening order.
    pub(crate) entries: Vec<Entry>,
    saved: Option<BTreeMap<String, Geometry>>,
    tick: u64,
}

impl Default for Windows {
    fn default() -> Self {
        Self {
            root: Root {
                id: WindowId::main(),
                title: None,
                sent_title: None,
                content: None,
                last_active: 0,
                geometry: None,
            },
            entries: Vec::new(),
            saved: None,
            tick: 0,
        }
    }
}

impl Windows {
    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    pub(crate) fn entry(&self, id: &WindowId) -> Option<&Entry> {
        self.entries.iter().find(|e| &e.id == id && !e.closed)
    }

    fn entry_mut(&mut self, id: &WindowId) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| &e.id == id && !e.closed)
    }

    pub(crate) fn by_viewport(&self, viewport: ViewportId) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| !e.closed && e.id.deferred_viewport() == viewport)
    }

    pub(crate) fn viewport_of(&self, id: &WindowId) -> Option<ViewportId> {
        if &self.root.id == id {
            return Some(ViewportId::ROOT);
        }
        self.entry(id).map(|e| e.id.deferred_viewport())
    }

    /// The windows other than the main one, worked in last first.
    fn others_by_recency(&self) -> Vec<WindowId> {
        let mut open: Vec<&Entry> = self.entries.iter().filter(|e| !e.closed).collect();
        open.sort_by_key(|e| std::cmp::Reverse(e.last_active));
        open.into_iter().map(|e| e.id.clone()).collect()
    }
}

thread_local! {
    /// The thread the windows were opened on (the UI thread).
    static MAIN_THREAD: Cell<Option<ThreadId>> = const { Cell::new(None) };
    /// A deferred window's viewport to its application and window.
    static REGISTRY: RefCell<BTreeMap<ViewportId, (WeakApp, WindowId)>> =
        const { RefCell::new(BTreeMap::new()) };
}

fn on_main_thread() -> bool {
    MAIN_THREAD.with(|m| m.get() == Some(std::thread::current().id()))
}

/// The wgpu present mode a multi-window application should use: on
/// Wayland (`WAYLAND_DISPLAY` set), `AutoNoVsync`, as a FIFO present can
/// stall every window behind one surface; `AutoVsync` elsewhere.
#[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
pub fn wanted_present_mode(wayland: bool) -> eframe::wgpu::PresentMode {
    if wayland {
        eframe::wgpu::PresentMode::AutoNoVsync
    } else {
        eframe::wgpu::PresentMode::AutoVsync
    }
}

/// Sets the options' present mode for several windows
/// ([`wanted_present_mode`], Wayland read from the environment), unless
/// the application chose one other than eframe's default. Returns the
/// mode in force.
#[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
pub fn present_mode(options: &mut eframe::NativeOptions) -> eframe::wgpu::PresentMode {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    if options.wgpu_options.surface.present_mode == eframe::wgpu::PresentMode::AutoVsync {
        options.wgpu_options.surface.present_mode = wanted_present_mode(wayland);
    }
    options.wgpu_options.surface.present_mode
}

impl AppHandle {
    /// Opens an application window, or brings it to the front when one of
    /// that id is open (its content is then left as it was). `content`
    /// draws it each pass of its own; it may hold `Rc` state. The main
    /// window's id (`main`, or a window moved into it) cannot be opened
    /// again.
    pub fn open_window(
        &self,
        spec: WindowSpec,
        content: impl FnMut(&mut Ui, &mut WindowUi<'_>) + 'static,
    ) -> WindowId {
        MAIN_THREAD.with(|m| {
            if m.get().is_none() {
                m.set(Some(std::thread::current().id()));
            }
        });
        let id = spec.id.clone();
        let opened = {
            let mut w = self.windows_mut();
            if w.root.id == id {
                false
            } else if let Some(e) = w.entry_mut(&id) {
                e.raise = true;
                false
            } else {
                self.load_geometry(&mut w);
                let saved = w.saved.as_ref().and_then(|s| s.get(id.as_str()).copied());
                let tick = w.next_tick();
                let first = match saved {
                    Some(g) => (Some(Vec2::from(g.size)), g.position, g.maximized),
                    None => (spec.size, None, false),
                };
                // A window of this id closed earlier is forgotten.
                w.entries.retain(|e| e.id != id);
                w.entries.push(Entry {
                    id: id.clone(),
                    title: spec.title.clone(),
                    sent_title: Some(spec.title.clone()),
                    content: Some(Box::new(content)),
                    first,
                    min_size: spec.min_size,
                    geometry: saved,
                    focused: None,
                    last_active: tick,
                    dispatcher: Some(KeyDispatcher::new()),
                    raise: false,
                    closed: false,
                });
                true
            }
        };
        if opened {
            REGISTRY.with(|r| {
                r.borrow_mut()
                    .insert(id.deferred_viewport(), (self.downgrade(), id.clone()))
            });
            self.publish(
                "egui_pop",
                None,
                "window.opened",
                json!({ "window": id, "title": spec.title }),
            );
        }
        self.egui().request_repaint_of(ViewportId::ROOT);
        id
    }

    /// Closes an application window (the main one: as its close button
    /// does, another window moving into it when one is open).
    pub fn close_window(&self, id: &WindowId) {
        let is_root = self.windows_mut().root.id == *id;
        if is_root {
            self.egui()
                .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            return;
        }
        let closed = {
            let mut w = self.windows_mut();
            match w.entry_mut(id) {
                Some(e) => {
                    e.closed = true;
                    true
                }
                None => false,
            }
        };
        if closed {
            self.window_closed(id);
        }
    }

    /// Brings a window to the front and gives it the keyboard focus. The
    /// system may refuse: on Wayland a window cannot take the focus from
    /// another application's, and may only be marked as wanting it.
    pub fn raise_window(&self, id: &WindowId) {
        let viewport = {
            let mut w = self.windows_mut();
            if w.root.id == *id {
                Some(ViewportId::ROOT)
            } else if let Some(e) = w.entry_mut(id) {
                e.raise = true;
                Some(e.id.deferred_viewport())
            } else {
                None
            }
        };
        if let Some(v) = viewport {
            let ctx = self.egui();
            ctx.send_viewport_cmd_to(v, ViewportCommand::Focus);
            ctx.request_repaint_of(v);
        }
    }

    /// The open windows: the main one first, then the others in opening
    /// order.
    pub fn windows(&self) -> Vec<WindowInfo> {
        let active = self.active_window();
        let w = self.windows_ref();
        let root_title = w.root.title.clone().unwrap_or_else(|| self.name());
        let mut out = vec![WindowInfo {
            id: w.root.id.clone(),
            title: root_title,
            main: true,
            active: active == ViewportId::ROOT,
            focused: self.egui().input(|i| {
                i.raw
                    .viewports
                    .get(&ViewportId::ROOT)
                    .and_then(|v| v.focused)
            }),
            geometry: w.root.geometry,
            viewport: ViewportId::ROOT,
        }];
        for e in w.entries.iter().filter(|e| !e.closed) {
            let viewport = e.id.deferred_viewport();
            out.push(WindowInfo {
                id: e.id.clone(),
                title: e.title.clone(),
                main: false,
                active: active == viewport,
                focused: e.focused,
                geometry: e.geometry,
                viewport,
            });
        }
        out
    }

    /// The application window drawn in a viewport, if one is.
    pub fn window_at(&self, viewport: ViewportId) -> Option<WindowId> {
        let w = self.windows_ref();
        if viewport == ViewportId::ROOT {
            return Some(w.root.id.clone());
        }
        w.by_viewport(viewport).map(|e| e.id.clone())
    }

    /// The viewport an open application window is drawn in.
    pub fn window_viewport(&self, id: &WindowId) -> Option<ViewportId> {
        self.windows_ref().viewport_of(id)
    }

    /// The application window the user worked in last (a detached
    /// panel's window counts as the main window's).
    pub fn active_app_window(&self) -> WindowId {
        let active = self.active_window();
        self.window_at(active)
            .unwrap_or_else(|| self.windows_ref().root.id.clone())
    }

    /// Sets a window's title (the content of a window usually sets its
    /// own, through [`WindowUi::set_title`]). The main window's title is
    /// the application's name until set.
    pub fn set_window_title(&self, id: &WindowId, title: impl Into<String>) {
        let title = title.into();
        let mut w = self.windows_mut();
        if w.root.id == *id {
            w.root.title = Some(title);
            drop(w);
            self.egui().request_repaint_of(ViewportId::ROOT);
        } else if let Some(e) = w.entry_mut(id) {
            e.title = title;
            let v = e.id.deferred_viewport();
            drop(w);
            self.egui().request_repaint_of(v);
        }
    }

    /// A window's geometry kept from an earlier run or seen in this one.
    pub fn window_geometry(&self, id: &WindowId) -> Option<Geometry> {
        let mut w = self.windows_mut();
        if w.root.id == *id {
            return w.root.geometry;
        }
        if let Some(e) = w.entry(id) {
            return e.geometry;
        }
        self.load_geometry(&mut w);
        w.saved.as_ref().and_then(|s| s.get(id.as_str()).copied())
    }

    pub(crate) fn windows_ref(&self) -> std::cell::Ref<'_, Windows> {
        self.0.windows.borrow()
    }

    pub(crate) fn windows_mut(&self) -> std::cell::RefMut<'_, Windows> {
        self.0.windows.borrow_mut()
    }

    fn load_geometry(&self, w: &mut Windows) {
        if w.saved.is_some() {
            return;
        }
        let saved = self
            .persist()
            .and_then(|p| std::fs::read_to_string(p.path(GEOMETRY_DOC)).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        w.saved = Some(saved);
    }

    /// Notes a window's geometry, saved a short while later.
    fn keep_geometry(&self, id: &WindowId, g: Geometry) {
        let text = {
            let mut w = self.windows_mut();
            self.load_geometry(&mut w);
            let saved = w.saved.get_or_insert_with(BTreeMap::new);
            if saved.get(id.as_str()) == Some(&g) {
                return;
            }
            saved.insert(id.to_string(), g);
            serde_json::to_string_pretty(saved).unwrap_or_default()
        };
        self.save_later(GEOMETRY_DOC, text);
    }

    fn window_closed(&self, id: &WindowId) {
        REGISTRY.with(|r| r.borrow_mut().remove(&id.deferred_viewport()));
        self.publish("egui_pop", None, "window.closed", json!({ "window": id }));
        let ctx = self.egui();
        ctx.request_repaint_of(ViewportId::ROOT);
    }

    /// The main window's close was asked while other windows are open:
    /// the window worked in last moves into it. Returns whether one did
    /// (the close is then to be cancelled).
    pub(crate) fn move_into_root(&self) -> bool {
        let moved = {
            let mut w = self.windows_mut();
            let Some(id) = w.others_by_recency().into_iter().next() else {
                return false;
            };
            let Some(at) = w.entries.iter().position(|e| e.id == id && !e.closed) else {
                return false;
            };
            let e = w.entries.remove(at);
            let left = std::mem::replace(&mut w.root.id, e.id.clone());
            w.root.title = Some(e.title.clone());
            w.root.sent_title = None;
            w.root.content = e.content;
            w.root.last_active = e.last_active;
            (left, e.id)
        };
        let (left, id) = moved;
        REGISTRY.with(|r| r.borrow_mut().remove(&id.deferred_viewport()));
        self.publish("egui_pop", None, "window.closed", json!({ "window": left }));
        self.publish(
            "egui_pop",
            None,
            "window.moved",
            json!({ "window": id, "to": "main" }),
        );
        self.egui().request_repaint_of(ViewportId::ROOT);
        true
    }

    /// Whether the main window shows a window's content moved into it
    /// rather than the application's shell.
    pub(crate) fn root_has_content(&self) -> bool {
        self.windows_ref().root.content.is_some()
    }

    /// The main window's part of the pass: its title, its geometry, and
    /// the content moved into it, if any (drawn in `ui`; `false` when the
    /// shell is to be drawn).
    pub(crate) fn root_pass(&self, ui: &mut Ui) -> bool {
        let ctx = ui.ctx().clone();
        if let Some(g) = read_geometry(&ctx) {
            let id = {
                let mut w = self.windows_mut();
                w.root.geometry = Some(g);
                w.root.id.clone()
            };
            if ctx.input(|i| i.viewport().close_requested()) {
                self.keep_geometry(&id, g);
            }
        }
        if ctx.input(|i| i.viewport().focused == Some(true)) {
            let mut w = self.windows_mut();
            let t = w.next_tick();
            w.root.last_active = t;
        }
        let content = self.windows_mut().root.content.take();
        let drawn = match content {
            Some(mut content) => {
                let (id, title) = {
                    let w = self.windows_ref();
                    (w.root.id.clone(), w.root.title.clone().unwrap_or_default())
                };
                let mut wui = WindowUi {
                    app: self,
                    id,
                    title,
                    class: ViewportClass::Root,
                    close_requested: false,
                    keep_open: false,
                    close: false,
                };
                content(ui, &mut wui);
                let mut w = self.windows_mut();
                if w.root.content.is_none() {
                    w.root.content = Some(content);
                }
                w.root.title = Some(wui.title);
                drop(w);
                if wui.close {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                true
            }
            None => false,
        };
        let title = {
            let w = self.windows_ref();
            w.root
                .title
                .clone()
                .filter(|t| w.root.sent_title.as_ref() != Some(t))
        };
        if let Some(t) = title {
            ctx.send_viewport_cmd(ViewportCommand::Title(t.clone()));
            self.windows_mut().root.sent_title = Some(t);
        }
        drawn
    }

    /// Declares the open windows, from the main window's pass: a window
    /// not declared in a pass of its parent is closed by egui.
    pub(crate) fn declare_windows(&self, ctx: &Context) {
        if ctx.viewport_id() != ViewportId::ROOT {
            return;
        }
        let app_id = self.app_id();
        let builders: Vec<(ViewportId, ViewportBuilder)> = {
            let mut w = self.windows_mut();
            w.entries.retain(|e| !e.closed);
            w.entries
                .iter()
                .map(|e| {
                    let mut b = ViewportBuilder::default().with_title(e.title.clone());
                    if !app_id.is_empty() {
                        b = b.with_app_id(app_id.clone());
                    }
                    let (size, pos, maximized) = e.first;
                    if let Some(s) = size {
                        b = b.with_inner_size(s);
                    }
                    if let Some(p) = pos {
                        b = b.with_position(p);
                    }
                    if maximized {
                        b = b.with_maximized(true);
                    }
                    if let Some(m) = e.min_size {
                        b = b.with_min_inner_size(m);
                    }
                    (e.id.deferred_viewport(), b)
                })
                .collect()
        };
        for (viewport, builder) in builders {
            ctx.show_viewport_deferred(viewport, builder, move |ui, class| {
                window_pass(viewport, ui, class)
            });
        }
    }

    /// A deferred window's keys, dispatched in its own pass with its
    /// dispatcher; the commands run before the main window's next pass,
    /// in this window's context.
    fn dispatch_window_keys(&self, ctx: &Context, id: &WindowId) {
        let Some(mut dispatcher) = self
            .windows_mut()
            .entry_mut(id)
            .and_then(|e| e.dispatcher.take())
        else {
            return;
        };
        let keymap = self.shared_keymap();
        let mut cx = self.when_context();
        cx.set(CONTEXT_APP_WINDOW, id.as_str());
        let dispatched = if self.capturing_keys() {
            Vec::new()
        } else {
            dispatcher.process(ctx, &keymap, &cx)
        };
        if let Some(e) = self.windows_mut().entry_mut(id) {
            e.dispatcher = Some(dispatcher);
        }
        for d in dispatched {
            self.run_in_context(d.command, serde_json::Value::Null, cx.clone());
        }
    }
}

/// Reads a window's geometry from its own pass.
fn read_geometry(ctx: &Context) -> Option<Geometry> {
    ctx.input(|i| {
        let v = i.viewport();
        let size = v
            .inner_rect
            .map(|r| r.size())
            .unwrap_or_else(|| i.viewport_rect().size());
        if size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        Some(Geometry {
            size: [size.x, size.y],
            position: v.outer_rect.map(|r| [r.min.x, r.min.y]),
            maximized: v.maximized.unwrap_or(false),
        })
    })
}

/// A deferred window's pass: the shim from egui's `Send + Sync` callback
/// to the application's `Rc` state, through the thread-local registry.
fn window_pass(viewport: ViewportId, ui: &mut Ui, class: ViewportClass) {
    let on_main = on_main_thread();
    debug_assert!(on_main, "an application window drawn off the UI thread");
    if !on_main {
        log::error!(target: "egui_pop", "an application window drawn off the UI thread");
        return;
    }
    let Some((weak, id)) = REGISTRY.with(|r| r.borrow().get(&viewport).cloned()) else {
        return;
    };
    let Ok(app) = weak.get() else { return };
    let ctx = ui.ctx().clone();
    let own_window = class != ViewportClass::EmbeddedWindow;
    let (content, title, raise) = {
        let mut w = app.windows_mut();
        let tick = w.next_tick();
        let Some(e) = w.entry_mut(&id) else { return };
        let Some(content) = e.content.take() else {
            return;
        };
        if own_window {
            e.focused = ctx.input(|i| i.viewport().focused);
            if e.focused == Some(true) {
                e.last_active = tick;
            }
            if let Some(g) = read_geometry(&ctx) {
                e.geometry = Some(g);
            }
        }
        let raise = std::mem::take(&mut e.raise);
        (content, e.title.clone(), raise)
    };
    let mut content = content;
    if own_window {
        app.note_window(&ctx);
        egui_pop_core::overlay::block_window_among(&ctx, &|v| v == viewport);
        egui_pop_core::overlay::begin_pass(&ctx);
        app.dispatch_window_keys(&ctx, &id);
        if raise {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }
    if let Some((from, to)) = egui_pop_core::scale::changed(&ctx) {
        app.publish(
            "egui_pop",
            None,
            "scale.changed",
            json!({ "viewport": id, "from": from, "to": to }),
        );
    }
    let close_requested = own_window && ctx.input(|i| i.viewport().close_requested());
    let mut wui = WindowUi {
        app: &app,
        id: id.clone(),
        title,
        class,
        close_requested,
        keep_open: false,
        close: false,
    };
    content(ui, &mut wui);
    if own_window {
        app.show_zone(ui, &crate::zones::WINDOW_OVERLAYS);
    }
    let closing = wui.close || (close_requested && !wui.keep_open);
    let (send_title, geometry) = {
        let mut w = app.windows_mut();
        let Some(e) = w.entry_mut(&id) else { return };
        e.content = Some(content);
        e.title = wui.title;
        let send = (e.sent_title.as_ref() != Some(&e.title)).then(|| e.title.clone());
        if send.is_some() {
            e.sent_title = send.clone();
        }
        if closing {
            e.closed = true;
        }
        (send, e.geometry)
    };
    if let Some(t) = send_title
        && own_window
    {
        ctx.send_viewport_cmd(ViewportCommand::Title(t));
    }
    if closing {
        if let Some(g) = geometry {
            app.keep_geometry(&id, g);
        }
        app.window_closed(&id);
    }
    if app.has_requests() {
        ctx.request_repaint_of(ViewportId::ROOT);
    }
}

/// The application's windows, for a test or a host: whether a viewport
/// is one of an application's windows, so that a modal open there blocks
/// it alone.
pub(crate) fn is_app_window(app: &AppHandle, viewport: ViewportId) -> bool {
    viewport != ViewportId::ROOT && app.windows_ref().by_viewport(viewport).is_some()
}

/// What the `app_windows` part of the `windows` collector shows.
pub(crate) fn describe(app: &AppHandle) -> serde_json::Value {
    json!(app.windows())
}

/// Brings a window to the front: `{ "window": id }`, or, without one,
/// the next open window after the active one (cycling).
pub const CMD_SWITCH: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("window.switch");
/// Closes a window: `{ "window": id }`, or the active one.
pub const CMD_CLOSE: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("window.close");
/// The Window menu.
pub const MENU_WINDOW: egui_pop_model::ids::MenuId =
    egui_pop_model::ids::MenuId::from_static("app.menu.window");
/// The list of open windows, by title, in the Window menu.
pub const LIST_WINDOWS: &str = "app.windows";

/// The window menu and its commands, for an application opening windows
/// of its own: Window lists the open windows by title (the active one
/// ticked) and switches to one; `window.switch` and `window.close` are
/// offered to agents and the palette.
#[derive(Debug, Default)]
pub struct WindowsPlugin;

fn window_arg(app: &AppHandle, args: &serde_json::Value) -> Result<Option<WindowId>, String> {
    match args.get("window").and_then(|w| w.as_str()) {
        None => Ok(None),
        Some(w) => {
            let id = WindowId::new(w);
            if app.window_viewport(&id).is_none() {
                return Err(format!("no open window `{w}`"));
            }
            Ok(Some(id))
        }
    }
}

impl crate::Plugin for WindowsPlugin {
    fn id(&self) -> egui_pop_model::ids::PluginId {
        egui_pop_model::ids::PluginId::from_static("egui_pop.windows")
    }

    fn title(&self) -> String {
        "Windows".into()
    }

    fn connect(&mut self, ctx: &mut crate::Ctx<'_>) -> Result<(), crate::PluginError> {
        use egui_pop_model::commands::CommandSpec;
        use egui_pop_model::menus::MenuItem;
        ctx.command(
            CommandSpec::new(CMD_SWITCH, "Switch window", "Window")
                .description("Brings a window to the front: the one named, or the next one.")
                .args(json!({ "window": "main" })),
            |call| {
                let app = call.app;
                let id = match window_arg(app, call.args)? {
                    Some(id) => id,
                    None => {
                        let open = app.windows();
                        let at = open.iter().position(|w| w.active).unwrap_or(0);
                        open[(at + 1) % open.len()].id.clone()
                    }
                };
                app.raise_window(&id);
                Ok(json!({ "window": id }))
            },
        )?
        .command(
            CommandSpec::new(CMD_CLOSE, "Close window", "Window")
                .description("Closes a window: the one named, or the active one.")
                .args(json!({ "window": "main" })),
            |call| {
                let app = call.app;
                let id = window_arg(app, call.args)?.unwrap_or_else(|| app.active_app_window());
                app.close_window(&id);
                Ok(json!({ "window": id }))
            },
        )?;
        ctx.submenu(crate::app::MENU_MAIN, "2_window", 0, MENU_WINDOW, "Window")?;
        ctx.menu_item(MenuItem::command(MENU_WINDOW, "1_close", 0, CMD_CLOSE))?;
        ctx.menu_item(MenuItem::list(MENU_WINDOW, "2_windows", 0, LIST_WINDOWS))?;
        ctx.list_provider(LIST_WINDOWS, |app, _| {
            app.windows()
                .into_iter()
                .map(|w| {
                    crate::menus::ListItem::new(&w.title, CMD_SWITCH, json!({ "window": w.id }))
                        .checked(w.active)
                })
                .collect()
        });
        Ok(())
    }
}
