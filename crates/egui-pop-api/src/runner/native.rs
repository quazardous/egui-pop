//! The event loop's handler (see the parent module).

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use eframe::UserEvent;
use egui::{Rect, ViewportId};
use egui_pop_winit::{Display, WindowHandles};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::WindowId;

/// A window eframe painted, and its viewport.
struct Paired {
    viewport: ViewportId,
    window: WindowId,
    /// Its handles were recorded in the per-viewport registries.
    recorded: bool,
}

thread_local! {
    /// winit allows one event loop per process: kept for a second run, as
    /// eframe does for its own.
    static EVENT_LOOP: RefCell<Option<EventLoop<UserEvent>>> = const { RefCell::new(None) };
    /// The window eframe is painting (set in its `RedrawRequested`, taken
    /// by the first pass that begins in it).
    static PAINTING: Cell<Option<WindowId>> = const { Cell::new(None) };
    static DISPLAY: Cell<Option<Display>> = const { Cell::new(None) };
    static SCREENS: RefCell<Vec<Rect>> = const { RefCell::new(Vec::new()) };
    static WINDOWS: RefCell<Vec<Paired>> = const { RefCell::new(Vec::new()) };
    /// The main window's handle as eframe gives it.
    static ROOT: Cell<Option<RawWindowHandle>> = const { Cell::new(None) };
    static VERIFIED: Cell<Option<bool>> = const { Cell::new(None) };
    /// The application's context, to forget a destroyed window's handles.
    static CTX: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

pub(super) fn screens() -> Vec<Rect> {
    SCREENS.with_borrow(Clone::clone)
}

pub(super) fn ids_verified() -> Option<bool> {
    VERIFIED.get()
}

fn handles(viewport: ViewportId) -> Option<WindowHandles> {
    if VERIFIED.get() != Some(true) {
        return None;
    }
    let window =
        WINDOWS.with_borrow(|w| w.iter().find(|p| p.viewport == viewport).map(|p| p.window))?;
    DISPLAY.get()?.window(window)
}

pub(super) fn window_handle(viewport: ViewportId) -> Option<RawWindowHandle> {
    handles(viewport).map(|h| h.raw_window())
}

/// The `on_begin_pass` hook: pairs the window being painted with the
/// pass's viewport, verifies the ids on the main window, and records the
/// handles of every paired window once they are verified.
fn note_viewport(ui: &mut egui::Ui) {
    let Some(window) = PAINTING.take() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let viewport = ctx.viewport_id();
    let new = WINDOWS.with_borrow_mut(|w| {
        if w.iter()
            .any(|p| p.window == window || p.viewport == viewport)
        {
            return false;
        }
        w.push(Paired {
            viewport,
            window,
            recorded: false,
        });
        true
    });
    if !new {
        return;
    }
    let Some(display) = DISPLAY.get() else {
        return;
    };
    log::debug!(
        target: "egui_pop::runner",
        "viewport {viewport:?} is window {:#x}",
        u64::from(window)
    );
    if viewport == ViewportId::ROOT && VERIFIED.get().is_none() {
        let from_id = display.window_handle(window);
        let ok = from_id.is_some() && from_id == ROOT.get();
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
    if VERIFIED.get() == Some(true) {
        record_all(&ctx, display);
    }
}

/// Records the handles of the paired windows not recorded yet. The main
/// window's are eframe's own, recorded at creation.
fn record_all(ctx: &egui::Context, display: Display) {
    let todo: Vec<(ViewportId, WindowId)> = WINDOWS.with_borrow_mut(|w| {
        w.iter_mut()
            .filter(|p| !p.recorded)
            .map(|p| {
                p.recorded = true;
                (p.viewport, p.window)
            })
            .collect()
    });
    for (viewport, window) in todo {
        if viewport == ViewportId::ROOT {
            continue;
        }
        let Some(h) = display.window(window) else {
            continue;
        };
        if let RawWindowHandle::Win32(w) = h.raw_window() {
            crate::frame::set_native_window(ctx, viewport, w.hwnd.get());
        }
        let parent: Arc<dyn crate::files::ParentWindow> = Arc::new(h);
        crate::files::set_parent_window(ctx, viewport, parent);
    }
}

/// A destroyed window's handles go.
fn forget(window: WindowId) {
    let gone: Vec<Paired> = WINDOWS.with_borrow_mut(|w| {
        let (gone, kept) = std::mem::take(w)
            .into_iter()
            .partition(|p| p.window == window);
        *w = kept;
        gone
    });
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

/// The loop's handler: egui-pop first, eframe for everything.
struct Runner<'a> {
    eframe: eframe::EframeWinitApplication<'a>,
}

impl ApplicationHandler<UserEvent> for Runner<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        DISPLAY.set(Display::of(event_loop));
        let screens: Vec<Rect> = event_loop
            .available_monitors()
            .map(|m| {
                let p = m.position();
                let s = m.size();
                super::screen_rect((p.x, p.y), (s.width, s.height), m.scale_factor())
            })
            .collect();
        log::debug!(target: "egui_pop::runner", "screens before the main window: {screens:?}");
        SCREENS.set(screens);
        self.eframe.resumed(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                let previous = PAINTING.replace(Some(id));
                self.eframe.window_event(event_loop, id, event);
                PAINTING.set(previous);
            }
            WindowEvent::Destroyed => {
                forget(id);
                self.eframe.window_event(event_loop, id, event);
            }
            event => self.eframe.window_event(event_loop, id, event),
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.eframe.new_events(event_loop, cause);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        self.eframe.user_event(event_loop, event);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, device: DeviceId, event: DeviceEvent) {
        self.eframe.device_event(event_loop, device, event);
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

/// What became of the application's creation, for the run's result:
/// eframe keeps its own error inside the handler `create_native` returns.
#[derive(Default)]
enum Made {
    #[default]
    NotCalled,
    Made,
    Failed(eframe::Error),
}

/// An error eframe gets in place of the one kept for the caller.
#[derive(Debug)]
struct Reported(String);

impl std::fmt::Display for Reported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Reported {}

pub(super) fn run(
    title: &str,
    mut options: eframe::NativeOptions,
    creator: eframe::AppCreator<'_>,
) -> eframe::Result {
    // A new run starts clean (a second run in the same process).
    PAINTING.set(None);
    ROOT.set(None);
    VERIFIED.set(None);
    WINDOWS.with_borrow_mut(Vec::clear);
    let hook = options.event_loop_builder.take();
    let built = EVENT_LOOP.with_borrow_mut(|slot| -> eframe::Result {
        if slot.is_none() {
            let mut builder = EventLoop::<UserEvent>::with_user_event();
            if let Some(hook) = hook {
                hook(&mut builder);
            }
            *slot = Some(builder.build()?);
        }
        Ok(())
    });
    built?;
    let previous = options.window_builder.take();
    options.window_builder = Some(Box::new(move |vb| {
        let vb = match previous {
            Some(hook) => hook(vb),
            None => vb,
        };
        super::repair_place(vb, &screens())
    }));
    let made = std::rc::Rc::new(RefCell::new(Made::default()));
    let made_in = made.clone();
    let creator: eframe::AppCreator<'_> = Box::new(move |cc| {
        if let Ok(h) = cc.window_handle() {
            ROOT.set(Some(h.as_raw()));
        }
        CTX.set(Some(cc.egui_ctx.clone()));
        cc.egui_ctx
            .on_begin_pass("egui_pop::runner", Arc::new(note_viewport));
        match creator(cc) {
            Ok(app) => {
                *made_in.borrow_mut() = Made::Made;
                Ok(app)
            }
            Err(e) => {
                let told: Box<dyn std::error::Error + Send + Sync> =
                    Box::new(Reported(e.to_string()));
                *made_in.borrow_mut() = Made::Failed(eframe::Error::AppCreation(e));
                Err(told)
            }
        }
    });
    let ran = EVENT_LOOP.with_borrow_mut(|slot| -> eframe::Result {
        let event_loop = slot.as_mut().expect("the event loop was built above");
        let app = eframe::create_native(title, options, creator, event_loop);
        let mut runner = Runner { eframe: app };
        event_loop.run_app_on_demand(&mut runner)?;
        Ok(())
    });
    CTX.set(None);
    WINDOWS.with_borrow_mut(Vec::clear);
    ran?;
    match std::mem::take(&mut *made.borrow_mut()) {
        Made::Made => Ok(()),
        Made::Failed(e) => Err(e),
        Made::NotCalled => Err(eframe::Error::AppCreation(Box::new(Reported(
            "eframe stopped before the application was made (its window or renderer could not start; see the log)".into(),
        )))),
    }
}
