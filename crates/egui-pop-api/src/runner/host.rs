//! What pairs each window with its viewport, for egui-pop's own runner
//! and for a host running the event loop itself.
//!
//! egui-pop's runner (`create-native`) calls these itself. A host that
//! builds the winit event loop and drives eframe through
//! `eframe::create_native` and `run_app_on_demand` (feature `host-loop`)
//! calls them from its own `ApplicationHandler`, so [`super::window`]
//! answers for every window there too, detached panels' windows
//! included:
//!
//! ```ignore
//! // In the application's creator, first:
//! egui_pop::api::runner::host::created(cc);
//!
//! impl ApplicationHandler<eframe::UserEvent> for HostLoop<'_> {
//!     fn resumed(&mut self, event_loop: &ActiveEventLoop) {
//!         egui_pop::api::runner::host::resumed(event_loop);
//!         self.eframe.resumed(event_loop);
//!     }
//!     fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
//!         egui_pop::api::runner::host::window_event(id, &event);
//!         if matches!(event, WindowEvent::RedrawRequested) {
//!             let eframe = &mut self.eframe;
//!             egui_pop::api::runner::host::redraw(id, || eframe.window_event(el, id, event));
//!         } else {
//!             self.eframe.window_event(el, id, event);
//!         }
//!     }
//!     // ... the other callbacks to eframe as before.
//! }
//! // After run_app_on_demand returns:
//! egui_pop::api::runner::host::detach();
//! ```
//!
//! [`redraw`] notes which window eframe paints: the pass that begins in
//! it is that window's viewport (a deferred viewport, the main window).
//! An immediate viewport is painted in a pass nested in its parent's,
//! and its own window's redraw runs no pass: the two are paired in the
//! order seen (see `pairs`). The handles come from the window's winit id
//! (`egui_pop_winit`), used only once the main window's id was seen to be
//! its native window ([`super::ids_verified`]).

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use egui::ViewportId;
use egui_pop_winit::{Display, WindowHandles};
use raw_window_handle::{HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::pairs::Pairs;
use crate::window::Windowing;

thread_local! {
    /// The window eframe is painting (set in its `RedrawRequested`, taken
    /// by the first pass that begins in it).
    static PAINTING: Cell<Option<WindowId>> = const { Cell::new(None) };
    /// Passes begun and not ended: more than one is a nested pass.
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static DISPLAY: Cell<Option<Display>> = const { Cell::new(None) };
    static PAIRS: RefCell<Pairs> = RefCell::new(Pairs::default());
    /// The main window's handle as eframe gives it.
    static ROOT: Cell<Option<RawWindowHandle>> = const { Cell::new(None) };
    static VERIFIED: Cell<Option<bool>> = const { Cell::new(None) };
    /// The main window, as eframe gives it at creation.
    static ROOT_WINDOW: RefCell<Option<super::Window>> = const { RefCell::new(None) };
    /// The application's context, to forget a destroyed window's handles.
    static CTX: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
    /// The main window's winit id.
    static ROOT_ID: Cell<Option<WindowId>> = const { Cell::new(None) };
}

/// In the application's creator (`eframe::AppCreator`), before anything
/// else: the main window (its handles and winit id) and the context
/// whose passes pair the other windows ([`attach`]).
pub fn created(cc: &eframe::CreationContext<'_>) {
    if let Ok(h) = cc.window_handle() {
        ROOT.set(Some(h.as_raw()));
    }
    if let Some(w) = cc.winit_window() {
        ROOT_ID.set(Some(w.id()));
        let w: Arc<dyn crate::files::ParentWindow> = w.clone();
        ROOT_WINDOW.set(Some(super::Window::new(w)));
    }
    attach(&cc.egui_ctx);
}

/// The main window, when [`created`] cannot give it (a host making its
/// own creation context, a test): its winit id and its handles.
pub fn set_main_window(id: WindowId, window: Arc<dyn crate::files::ParentWindow>) {
    ROOT.set(window.window_handle().ok().map(|h| h.as_raw()));
    ROOT_ID.set(Some(id));
    ROOT_WINDOW.set(Some(super::Window::new(window)));
}

/// Pairs the windows painted for `ctx`'s passes: hooks on its passes'
/// begin and end, once per context.
pub fn attach(ctx: &egui::Context) {
    let same = CTX.with_borrow(|c| c.as_ref().is_some_and(|c| c == ctx));
    if same {
        return;
    }
    CTX.set(Some(ctx.clone()));
    DEPTH.set(0);
    ctx.on_begin_pass("egui_pop::runner", Arc::new(begin_pass));
    ctx.on_end_pass(
        "egui_pop::runner",
        Arc::new(|_| DEPTH.set(DEPTH.get().saturating_sub(1))),
    );
}

/// When the event loop resumes, before eframe's `resumed`: the display
/// the windows' ids are read against ([`set_display`]).
pub fn resumed(event_loop: &ActiveEventLoop) {
    if let Some(d) = Display::of(event_loop) {
        DISPLAY.set(Some(d));
    }
}

/// The display the windows' ids are read against, when [`resumed`]
/// cannot give it (a test).
pub fn set_display(display: RawDisplayHandle) {
    DISPLAY.set(Some(Display::from_raw(display)));
}

/// Around eframe's handling of `window`'s `RedrawRequested`: the pass
/// that begins in it is that window's viewport; no pass at all, an
/// immediate viewport's window (eframe repaints its parent instead).
pub fn redraw<R>(window: WindowId, paint: impl FnOnce() -> R) -> R {
    let previous = PAINTING.replace(Some(window));
    let out = paint();
    let untaken = PAINTING.get() == Some(window);
    PAINTING.set(previous);
    if untaken && ROOT_ID.get() != Some(window) {
        let new = PAIRS.with_borrow_mut(|p| p.redraw_without_pass(u64::from(window)));
        if new {
            record_paired();
        }
    }
    out
}

/// Each window event, before eframe gets it: a destroyed window's
/// handles go.
pub fn window_event(window: WindowId, event: &WindowEvent) {
    if matches!(event, WindowEvent::Destroyed) {
        forget(window);
    }
}

/// After the loop returned: everything forgotten, for a next run.
pub fn detach() {
    PAINTING.set(None);
    DEPTH.set(0);
    CTX.set(None);
    ROOT_ID.set(None);
    ROOT_WINDOW.set(None);
    ROOT.set(None);
    VERIFIED.set(None);
    PAIRS.with_borrow_mut(Pairs::clear);
}

pub(super) fn main_window_id() -> Option<WindowId> {
    ROOT_ID.get()
}

pub(super) fn ids_verified() -> Option<bool> {
    VERIFIED.get()
}

pub(super) fn windowing() -> Windowing {
    match DISPLAY.get().map(|d| d.raw()) {
        None => Windowing::Unknown,
        Some(RawDisplayHandle::Wayland(_)) => Windowing::Wayland,
        Some(RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)) => Windowing::X11,
        Some(RawDisplayHandle::Windows(_)) => Windowing::Windows,
        Some(RawDisplayHandle::AppKit(_)) => Windowing::MacOs,
        Some(_) => Windowing::Other,
    }
}

fn handles(viewport: ViewportId) -> Option<WindowHandles> {
    if VERIFIED.get() != Some(true) {
        return None;
    }
    let window = PAIRS.with_borrow(|p| p.window_of(viewport))?;
    DISPLAY.get()?.window(WindowId::from(window))
}

pub(super) fn window_handle(viewport: ViewportId) -> Option<RawWindowHandle> {
    if viewport == ViewportId::ROOT
        && let Some(h) = ROOT.get()
    {
        return Some(h);
    }
    handles(viewport).map(|h| h.raw_window())
}

pub(super) fn window(viewport: ViewportId) -> Option<super::Window> {
    if viewport == ViewportId::ROOT {
        return ROOT_WINDOW.with_borrow(Clone::clone);
    }
    handles(viewport).map(|h| super::Window::new(Arc::new(h)))
}

pub(super) fn windows() -> Vec<ViewportId> {
    let mut known = Vec::new();
    if ROOT_WINDOW.with_borrow(Option::is_some) {
        known.push(ViewportId::ROOT);
    }
    if VERIFIED.get() == Some(true) && DISPLAY.get().is_some() {
        PAIRS.with_borrow(|p| {
            known.extend(p.viewports().filter(|v| *v != ViewportId::ROOT));
        });
    }
    known
}

/// The `on_begin_pass` hook: pairs the window being painted with the
/// pass's viewport (or, nested, notes an immediate viewport), verifies
/// the ids on the main window, and records the handles of every paired
/// window once they are verified.
fn begin_pass(ui: &mut egui::Ui) {
    let depth = DEPTH.get() + 1;
    DEPTH.set(depth);
    let ctx = ui.ctx().clone();
    let viewport = ctx.viewport_id();
    let new = if depth > 1 {
        PAIRS.with_borrow_mut(|p| p.nested(viewport))
    } else {
        let Some(window) = PAINTING.take() else {
            return;
        };
        let open: Vec<ViewportId> = ctx.input(|i| i.raw.viewports.keys().copied().collect());
        let new = PAIRS.with_borrow_mut(|p| {
            if viewport == ViewportId::ROOT && !open.is_empty() {
                p.retain_viewports(&open);
            }
            p.painted(u64::from(window), viewport)
        });
        if new {
            log::debug!(
                target: "egui_pop::runner",
                "viewport {viewport:?} is window {:#x}",
                u64::from(window)
            );
        }
        if new && viewport == ViewportId::ROOT && VERIFIED.get().is_none() {
            verify(window);
        }
        new
    };
    if new {
        record_paired();
    }
}

/// Whether winit's ids are the native windows: the main window's id read
/// as a handle against the handle eframe gave.
fn verify(window: WindowId) {
    let Some(display) = DISPLAY.get() else {
        return;
    };
    let from_id = display.window_handle(window);
    let ok = match (from_id, ROOT.get()) {
        (Some(a), Some(b)) => egui_pop_winit::same_window(&a, &b),
        _ => false,
    };
    VERIFIED.set(Some(ok));
    if ok {
        log::debug!(target: "egui_pop::runner", "window ids are the native windows");
    } else {
        log::info!(
            target: "egui_pop::runner",
            "window ids are not the native windows here ({from_id:?} against {:?}): only the main window's handles are known",
            ROOT.get()
        );
    }
}

/// Records the handles of the paired windows not recorded yet, once the
/// ids are verified. The main window's are eframe's own.
fn record_paired() {
    if VERIFIED.get() != Some(true) {
        return;
    }
    let (Some(display), Some(ctx)) = (DISPLAY.get(), CTX.with_borrow(Clone::clone)) else {
        return;
    };
    let todo = PAIRS.with_borrow_mut(Pairs::take_unrecorded);
    for pair in todo {
        if pair.viewport == ViewportId::ROOT {
            continue;
        }
        let Some(h) = display.window(WindowId::from(pair.window)) else {
            continue;
        };
        log::debug!(
            target: "egui_pop::runner",
            "viewport {:?} is window {:#x}{}",
            pair.viewport,
            pair.window,
            if pair.guessed { " (immediate)" } else { "" }
        );
        if let RawWindowHandle::Win32(w) = h.raw_window() {
            crate::frame::set_native_window(&ctx, pair.viewport, w.hwnd.get());
        }
        let parent: Arc<dyn crate::files::ParentWindow> = Arc::new(h);
        crate::files::set_parent_window(&ctx, pair.viewport, parent);
        ctx.request_repaint_of(pair.viewport);
    }
}

/// A destroyed window's handles go.
fn forget(window: WindowId) {
    let gone = PAIRS.with_borrow_mut(|p| p.forget(u64::from(window)));
    CTX.with_borrow(|ctx| {
        let Some(ctx) = ctx else { return };
        for p in gone
            .iter()
            .filter(|p| p.recorded && p.viewport != ViewportId::ROOT)
        {
            crate::files::forget_parent_window(ctx, p.viewport);
            crate::frame::forget_native_window(ctx, p.viewport);
        }
    });
}
