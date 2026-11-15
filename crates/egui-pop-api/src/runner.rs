//! Spike (step 0 of the eframe realignment, branch only, never merged):
//! eframe run through `create_native` on a winit event loop egui-pop owns,
//! to measure which native hooks that reaches without leaving eframe.
//!
//! What owning the loop gives, and how this module uses it:
//!
//! - **Every window event first.** [`Runner`] is the loop's
//!   `ApplicationHandler`; it sees each event (with its `WindowId`) before
//!   eframe does, and the `ActiveEventLoop` with it.
//! - **Monitors before placement.** `Runner::resumed` reads the screens
//!   before eframe's `resumed` creates the root window; eframe calls the
//!   root's `window_builder` hook inside that `resumed`, so the hook
//!   repairs the restored place against real screens.
//! - **Each viewport's window.** eframe paints a window synchronously in
//!   its `RedrawRequested`; `Runner` notes which window is painting, and a
//!   `Context::on_begin_pass` hook pairs it with `ctx.viewport_id()`.
//!   winit 0.30's `WindowId` *is* the native window on the desktop
//!   platforms (an `HWND` on Windows, the X11 id, the `wl_surface` proxy on
//!   Wayland, the `NSWindow` on macOS): an implementation detail, not an
//!   API, which this module turns into raw window handles.

#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;
use std::sync::Arc;

use eframe::UserEvent;
use egui::{Pos2, Rect, Vec2, ViewportId};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandWindowHandle, Win32WindowHandle, WindowHandle, XcbWindowHandle,
    XlibWindowHandle,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

thread_local! {
    /// The window eframe is painting right now (inside its `RedrawRequested`).
    static PAINTING: Cell<Option<WindowId>> = const { Cell::new(None) };
    static DISPLAY: Cell<Option<RawDisplayHandle>> = const { Cell::new(None) };
    /// The screens in points, read before the root window is created.
    static SCREENS: RefCell<Vec<Rect>> = const { RefCell::new(Vec::new()) };
    static WINDOWS: RefCell<Vec<(ViewportId, WindowId)>> = const { RefCell::new(Vec::new()) };
}

/// The screens seen by the event loop, in points (empty before it runs).
pub fn screens() -> Vec<Rect> {
    SCREENS.with_borrow(Clone::clone)
}

/// The native window of a viewport, once it has been painted.
pub fn window_handle(viewport: ViewportId) -> Option<RawWindowHandle> {
    let id = WINDOWS.with_borrow(|w| w.iter().find(|(v, _)| *v == viewport).map(|(_, w)| *w))?;
    raw_from_id(id.into(), DISPLAY.get()?)
}

/// winit 0.30's window id as a raw handle, per windowing system.
fn raw_from_id(id: u64, display: RawDisplayHandle) -> Option<RawWindowHandle> {
    match display {
        RawDisplayHandle::Wayland(_) => NonNull::new(id as usize as *mut c_void)
            .map(|p| RawWindowHandle::Wayland(WaylandWindowHandle::new(p))),
        #[allow(clippy::useless_conversion)]
        RawDisplayHandle::Xlib(_) => Some(RawWindowHandle::Xlib(XlibWindowHandle::new(
            id.try_into().ok()?,
        ))),
        RawDisplayHandle::Xcb(_) => NonZeroU32::new(u32::try_from(id).ok()?)
            .map(|w| RawWindowHandle::Xcb(XcbWindowHandle::new(w))),
        RawDisplayHandle::Windows(_) => NonZeroIsize::new(id as isize)
            .map(|h| RawWindowHandle::Win32(Win32WindowHandle::new(h))),
        // AppKit's handle is the NSView; the id is the NSWindow
        // (`contentView` would need objc2): not done in the spike.
        _ => None,
    }
}

/// A viewport's raw handles, for what takes `HasWindowHandle` (the system
/// file chooser's parent, the Wayland image subsurface).
struct ViewportWindow {
    window: RawWindowHandle,
    display: RawDisplayHandle,
}

// SAFETY: raw handles of a window eframe keeps alive while its viewport
// lives; only read on the UI thread, as `files` does with eframe's own.
unsafe impl Send for ViewportWindow {}
// SAFETY: as above.
unsafe impl Sync for ViewportWindow {}

impl HasWindowHandle for ViewportWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: see `Send` above.
        Ok(unsafe { WindowHandle::borrow_raw(self.window) })
    }
}

impl HasDisplayHandle for ViewportWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        // SAFETY: the display outlives every window of the loop.
        Ok(unsafe { DisplayHandle::borrow_raw(self.display) })
    }
}

/// Pairs the window being painted with the pass's viewport, the first time,
/// and records its handles where egui-pop keeps them per viewport.
fn note_viewport(ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    let Some(window) = PAINTING.get() else {
        return;
    };
    let viewport = ctx.viewport_id();
    let new = WINDOWS.with_borrow_mut(|w| {
        // An immediate viewport's pass runs inside its parent's paint: the
        // window is already someone else's.
        if w.iter().any(|(v, id)| *id == window || *v == viewport) {
            return false;
        }
        w.push((viewport, window));
        true
    });
    if !new {
        return;
    }
    let Some(display) = DISPLAY.get() else {
        return;
    };
    let id: u64 = window.into();
    let raw = raw_from_id(id, display);
    eprintln!("egui-pop-runner: viewport {viewport:?} is window {id:#x}: {raw:?}");
    let Some(raw) = raw else { return };
    if viewport != ViewportId::ROOT {
        let parent: Arc<dyn crate::files::ParentWindow> = Arc::new(ViewportWindow {
            window: raw,
            display,
        });
        crate::files::set_parent_window(ctx, viewport, parent);
    }
    if let RawWindowHandle::Win32(w) = raw {
        crate::frame::set_native_window(ctx, viewport, w.hwnd.get());
    }
}

/// The root's restored place, repaired against the real screens.
fn repair_place(mut vb: egui::ViewportBuilder) -> egui::ViewportBuilder {
    let screens = screens();
    let Some(size) = vb.inner_size else { return vb };
    let state = crate::window::WindowState {
        position: vb.position,
        size,
        maximized: false,
        fullscreen: false,
    }
    .repair(&screens);
    eprintln!(
        "egui-pop-runner: root place {:?} {:?} repaired on {} screen(s): {:?}",
        vb.position,
        size,
        screens.len(),
        state.position
    );
    vb.position = state.position;
    vb.inner_size = Some(state.size);
    vb
}

/// The loop's handler: egui-pop first, eframe for everything.
struct Runner<'a> {
    eframe: eframe::EframeWinitApplication<'a>,
}

impl ApplicationHandler<UserEvent> for Runner<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Ok(d) = event_loop.display_handle() {
            DISPLAY.set(Some(d.as_raw()));
        }
        let screens: Vec<Rect> = event_loop
            .available_monitors()
            .map(|m| {
                let s = m.scale_factor() as f32;
                let p = m.position();
                let z = m.size();
                Rect::from_min_size(
                    Pos2::new(p.x as f32 / s, p.y as f32 / s),
                    Vec2::new(z.width as f32 / s, z.height as f32 / s),
                )
            })
            .collect();
        eprintln!("egui-pop-runner: screens before the root window: {screens:?}");
        SCREENS.set(screens);
        self.eframe.resumed(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            let previous = PAINTING.replace(Some(id));
            self.eframe.window_event(event_loop, id, event);
            PAINTING.set(previous);
        } else {
            if let WindowEvent::ActivationTokenDone { .. } = &event {
                eprintln!("egui-pop-runner: activation token for window {:#x}", u64::from(id));
            }
            if matches!(event, WindowEvent::Destroyed) {
                WINDOWS.with_borrow_mut(|w| w.retain(|(_, w)| *w != id));
            }
            self.eframe.window_event(event_loop, id, event);
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        self.eframe.new_events(event_loop, cause);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        self.eframe.user_event(event_loop, event);
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.eframe.device_event(event_loop, device_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.about_to_wait(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.suspended(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.memory_warning(event_loop);
    }
}

/// `eframe::run_native`, on an event loop egui-pop owns.
pub(crate) fn run(
    title: &str,
    mut options: eframe::NativeOptions,
    creator: eframe::AppCreator<'_>,
) -> eframe::Result {
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    if let Some(hook) = options.event_loop_builder.take() {
        hook(&mut builder);
    }
    let event_loop = builder.build()?;
    let previous = options.window_builder.take();
    options.window_builder = Some(Box::new(move |vb| {
        repair_place(match previous {
            Some(hook) => hook(vb),
            None => vb,
        })
    }));
    let creator: eframe::AppCreator<'_> = Box::new(move |cc| {
        cc.egui_ctx
            .on_begin_pass("egui-pop runner", Arc::new(note_viewport));
        creator(cc)
    });
    let app = eframe::create_native(title, options, creator, &event_loop);
    event_loop.run_app(&mut Runner { eframe: app })?;
    Ok(())
}
