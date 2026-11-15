//! Guards the assumption the crate rests on: winit 0.30's `WindowId` is
//! the native window. A real (hidden) window is created on whatever
//! system the test runs, and the handle read from its id must be the
//! handle winit gives for it: on Windows (the `HWND`), X11 (the window
//! id) and Wayland (the `wl_surface`). On macOS the crate gives none, and
//! must keep saying so. Without a display (a headless CI runner) there is
//! no window to create: the test says so and passes.
//!
//! Its own `main` (`harness = false`): macOS creates windows on the main
//! thread only. It answers the slice of libtest's command line
//! cargo-nextest uses (`--list`, a name filter, `--ignored`).

use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct Probe {
    outcome: Option<Result<String, String>>,
}

impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.outcome.is_some() {
            return;
        }
        self.outcome = Some(check(event_loop));
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn check(event_loop: &ActiveEventLoop) -> Result<String, String> {
    let window: Window = event_loop
        .create_window(
            Window::default_attributes()
                .with_visible(false)
                .with_title("egui-pop-winit window id test"),
        )
        .map_err(|e| format!("no window: {e}"))?;
    let display = egui_pop_winit::Display::of(event_loop).ok_or("no display handle")?;
    let real = window
        .window_handle()
        .map_err(|e| format!("winit gives no handle: {e}"))?
        .as_raw();
    let from_id = display.window_handle(window.id());
    let macos = cfg!(target_os = "macos");
    match (from_id, macos) {
        (None, true) => Ok("macOS: no handle from the id, as documented".into()),
        (Some(h), false) if egui_pop_winit::same_window(&h, &real) => {
            let handles = display.window(window.id()).ok_or("no handles")?;
            let borrowed = handles
                .window_handle()
                .map_err(|e| format!("handles not borrowed: {e}"))?
                .as_raw();
            if egui_pop_winit::same_window(&borrowed, &real) {
                Ok(format!("the id is the window: {real:?}"))
            } else {
                Err(format!("borrowed {borrowed:?}, winit gives {real:?}"))
            }
        }
        (got, _) => Err(format!(
            "window id {:#x} read as {got:?} on {:?}; winit gives {real:?}",
            u64::from(window.id()),
            display.raw()
        )),
    }
}

/// The slice of libtest's command line cargo-nextest relies on: it lists
/// a test binary with `--list --format terse` (and again with
/// `--ignored`), expecting `name: test` lines, then runs each test by
/// name. Returns whether the test is to run.
fn libtest_cli() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ignored = args.iter().any(|a| a == "--ignored");
    if args.iter().any(|a| a == "--list") {
        if !ignored {
            println!("window_id: test");
        }
        return false;
    }
    // A name filter (an argument that is neither an option nor an
    // option's value) that does not match this test runs nothing.
    let mut filters = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if matches!(
            a.as_str(),
            "--format" | "--test-threads" | "--color" | "--skip" | "-Z"
        ) {
            it.next();
        } else if !a.starts_with('-') {
            filters.push(a.as_str());
        }
    }
    let exact = args.iter().any(|a| a == "--exact");
    let matches = filters.is_empty()
        || filters.iter().any(|f| {
            if exact {
                *f == "window_id"
            } else {
                "window_id".contains(f)
            }
        });
    !ignored && matches
}

fn main() {
    if !libtest_cli() {
        return;
    }
    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            println!("window_id: skipped, no display ({e})");
            return;
        }
    };
    let mut probe = Probe::default();
    if let Err(e) = event_loop.run_app(&mut probe) {
        println!("window_id: skipped, the event loop did not run ({e})");
        return;
    }
    match probe.outcome {
        Some(Ok(what)) => println!("window_id: ok, {what}"),
        Some(Err(why)) => {
            eprintln!("window_id: FAILED, {why}");
            std::process::exit(1);
        }
        None => println!("window_id: skipped, the event loop never resumed"),
    }
}
