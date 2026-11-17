//! The event loop's handler (see the parent module).

mod pace;

use std::cell::RefCell;
use std::time::Instant;

use eframe::UserEvent;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::WindowId;

use super::host;
use crate::main_wake::{MainWake, PassCounter};

thread_local! {
    /// winit allows one event loop per process: kept for a second run, as
    /// eframe does for its own.
    static EVENT_LOOP: RefCell<Option<EventLoop<UserEvent>>> = const { RefCell::new(None) };
    /// The application's context, for the main window's owed frames.
    static CTX: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
    /// The context's passes, counted (`main_wake`).
    static PASSES: RefCell<Option<PassCounter>> = const { RefCell::new(None) };
}

/// Tells egui-pop the monitors (`window::set_screens`), named as winit
/// names them; only when they changed.
fn tell_screens(event_loop: &ActiveEventLoop) {
    let screens: Vec<crate::window::Screen> = event_loop
        .available_monitors()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            crate::window::Screen::from_physical(
                m.name(),
                (p.x, p.y),
                (s.width, s.height),
                m.scale_factor(),
            )
        })
        .collect();
    if crate::window::screen_list() != screens {
        crate::window::set_screens(screens);
    }
}

/// The loop's handler: egui-pop first, eframe for everything.
struct Runner<'a> {
    eframe: eframe::EframeWinitApplication<'a>,
    /// Each window's paints, at most one per interval (see `pace`).
    pacer: pace::Pacer<WindowId>,
    /// The main window's frames owed, painted when the system withholds
    /// its redraw (`main_wake`).
    wake: MainWake<WindowId>,
}

impl Runner<'_> {
    /// Paints `window` now: eframe runs its pass in `RedrawRequested`.
    fn paint(&mut self, event_loop: &ActiveEventLoop, window: WindowId) {
        if self.wake.main().is_none()
            && let Some(main) = host::main_window_id()
        {
            self.wake.set_main(main);
            if let Some(c) = PASSES.with_borrow(Clone::clone) {
                self.wake.set_counter(c);
            }
        }
        let eframe = &mut self.eframe;
        let wake = &mut self.wake;
        host::redraw(window, || {
            wake.redraw(window, || {
                eframe.window_event(event_loop, window, WindowEvent::RedrawRequested);
            });
        });
    }

    /// Paints the windows whose held redraw came due, and the main window
    /// when its frame is owed past the grace.
    fn paint_due(&mut self, event_loop: &ActiveEventLoop) {
        for window in self.pacer.due(Instant::now()) {
            log::trace!(target: "egui_pop::runner", "held redraw of {window:?} painted");
            self.paint(event_loop, window);
        }
        if let Some(main) = self.wake.due(Instant::now()) {
            log::debug!(target: "egui_pop::runner", "the main window's redraw is withheld: painted");
            self.paint(event_loop, main);
        }
    }
}

impl ApplicationHandler<UserEvent> for Runner<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        host::resumed(event_loop);
        tell_screens(event_loop);
        let interval = pace::interval(
            event_loop
                .available_monitors()
                .filter_map(|m| m.refresh_rate_millihertz()),
        );
        log::debug!(target: "egui_pop::runner", "frame interval: {interval:?}");
        self.pacer.set_interval(interval);
        self.eframe.resumed(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                if self.pacer.arrive(id, Instant::now()) {
                    self.paint(event_loop, id);
                }
            }
            WindowEvent::ScaleFactorChanged { .. } | WindowEvent::Moved(_) => {
                tell_screens(event_loop);
                self.eframe.window_event(event_loop, id, event);
            }
            WindowEvent::Destroyed => {
                host::window_event(id, &event);
                self.pacer.forget(id);
                self.eframe.window_event(event_loop, id, event);
            }
            event => self.eframe.window_event(event_loop, id, event),
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.eframe.new_events(event_loop, cause);
        self.paint_due(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        if let Some(ctx) = CTX.with_borrow(Clone::clone) {
            self.wake.user_event(&ctx, &event);
        }
        self.eframe.user_event(event_loop, event);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, device: DeviceId, event: DeviceEvent) {
        self.eframe.device_event(event_loop, device, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.paint_due(event_loop);
        self.eframe.about_to_wait(event_loop);
        // Wake for the next held redraw or owed frame, if eframe would
        // sleep past it.
        let next = [self.pacer.next(), self.wake.next()]
            .into_iter()
            .flatten()
            .min();
        if let Some(at) = next {
            let wake = match event_loop.control_flow() {
                ControlFlow::Wait => Some(at),
                ControlFlow::WaitUntil(t) if at < t => Some(at),
                _ => None,
            };
            if let Some(at) = wake {
                event_loop.set_control_flow(ControlFlow::WaitUntil(at));
            }
        }
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
enum Creation {
    #[default]
    Pending,
    Done,
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
    host::detach();
    let wake_enabled = crate::main_wake::paint_safe(options.wgpu_options.surface.present_mode);
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
        crate::window::repair_builder(vb)
    }));
    let made = std::rc::Rc::new(RefCell::new(Creation::default()));
    let made_in = made.clone();
    let creator: eframe::AppCreator<'_> = Box::new(move |cc| {
        host::created(cc);
        CTX.set(Some(cc.egui_ctx.clone()));
        PASSES.set(Some(PassCounter::install(&cc.egui_ctx)));
        match creator(cc) {
            Ok(app) => {
                *made_in.borrow_mut() = Creation::Done;
                Ok(app)
            }
            Err(e) => {
                let told: Box<dyn std::error::Error + Send + Sync> =
                    Box::new(Reported(e.to_string()));
                *made_in.borrow_mut() = Creation::Failed(eframe::Error::AppCreation(e));
                Err(told)
            }
        }
    });
    let ran = EVENT_LOOP.with_borrow_mut(|slot| -> eframe::Result {
        let event_loop = slot.as_mut().expect("the event loop was built above");
        let app = eframe::create_native(title, options, creator, event_loop);
        let mut wake = MainWake::new();
        wake.set_enabled(wake_enabled);
        let mut runner = Runner {
            eframe: app,
            pacer: pace::Pacer::new(pace::interval([])),
            wake,
        };
        event_loop.run_app_on_demand(&mut runner)?;
        Ok(())
    });
    CTX.set(None);
    PASSES.set(None);
    host::detach();
    ran?;
    match std::mem::take(&mut *made.borrow_mut()) {
        Creation::Done => Ok(()),
        Creation::Failed(e) => Err(e),
        Creation::Pending => Err(eframe::Error::AppCreation(Box::new(Reported(
            "eframe stopped before the application was made (its window or renderer could not start; see the log)".into(),
        )))),
    }
}
