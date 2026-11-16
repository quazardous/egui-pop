//! The native runner: eframe on an event loop egui-pop owns.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! With the `create-native` feature (on by default), `AppBuilder::run_native`
//! and `run_native_with` do not call `eframe::run_native`: they build the
//! winit event loop, hand eframe to it through `eframe::create_native`, and
//! run it with a handler of their own, which sees every winit callback
//! before eframe does. eframe still creates and paints every window and
//! runs the application as before; what owning the loop adds:
//!
//! - **The screens before the main window.** The handler reads the
//!   monitors in `resumed`, before eframe's own `resumed` creates the main
//!   window, and tells them (`window::set_screens`, the way a host running
//!   its own loop tells them); eframe calls the window's `window_builder`
//!   hook inside it, so the place restored from the last run is repaired
//!   against real screens (`window::repair_builder`), not only bounded.
//!   They are read again when a window's scale factor changes or a window
//!   moves (onto another monitor, maybe). [`screens`] gives them.
//! - **Every window's handles.** eframe hands out the main window's only.
//!   It paints a window synchronously in that window's `RedrawRequested`,
//!   so the handler notes which window is being painted, and a
//!   `Context::on_begin_pass` hook pairs it with the pass's viewport. The
//!   handles come from the window's winit id (`egui_pop_winit`, where the
//!   `unsafe` lives), checked first against the main window's handles from
//!   eframe: they are used only once both agree ([`ids_verified`]). Each
//!   window's handles are recorded where egui-pop keeps them per viewport
//!   (`files::set_parent_window`, `frame::set_native_window`) and forgotten
//!   when winit destroys the window. [`window`] gives a viewport's
//!   window by its id, [`windows`] the viewports whose window is known,
//!   [`window_handle`] the raw handle alone.
//! - **Paced paints.** A window paints at most once per interval, a
//!   little under the fastest screen's refresh period (read in `resumed`,
//!   60 Hz when no screen tells): a `RedrawRequested` that comes sooner
//!   is held and painted at its time. Vsync and Wayland's frame callbacks
//!   pace a window already; this holds where the present does not wait
//!   (an X server without DRI3, Mesa's software rendering), where an
//!   animating window would otherwise draw as fast as the CPU goes.
//!
//! Immediate viewports (painted inside their parent's pass) are not
//! paired. On the web, and without the feature, eframe runs as before and
//! this module's functions answer nothing.

#[cfg(all(
    feature = "create-native",
    not(any(target_os = "ios", target_os = "android"))
))]
mod native;

/// Without the feature (or on iOS and Android): eframe's own runner, and
/// nothing seen.
#[cfg(not(all(
    feature = "create-native",
    not(any(target_os = "ios", target_os = "android"))
)))]
mod native {
    use egui::ViewportId;
    use raw_window_handle::RawWindowHandle;

    pub(super) fn window_handle(_: ViewportId) -> Option<RawWindowHandle> {
        None
    }

    pub(super) fn window(_: ViewportId) -> Option<super::Window> {
        None
    }

    pub(super) fn windows() -> Vec<ViewportId> {
        Vec::new()
    }

    pub(super) fn ids_verified() -> Option<bool> {
        None
    }

    pub(super) fn run(
        title: &str,
        options: eframe::NativeOptions,
        creator: eframe::AppCreator<'_>,
    ) -> eframe::Result {
        eframe::run_native(title, options, creator)
    }
}

use std::sync::Arc;

use egui::{Rect, ViewportId};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle, WindowHandle,
};

use crate::files::ParentWindow;

/// The handle traits [`Window`] implements.
pub use raw_window_handle;

/// The native window showing a viewport ([`window`]): what takes a window
/// through `HasWindowHandle` and `HasDisplayHandle` (a file chooser's
/// parent, a Wayland subsurface, a Win32 subclass) is given it.
///
/// The main window's is eframe's own window; another window's handles
/// are read from its winit id. They name a window winit keeps: use them
/// on the thread that runs the event loop (the one running the
/// application's frames), and keep them no longer than the viewport is
/// open; once it closes, [`window`] answers none for it.
#[derive(Clone)]
pub struct Window(Arc<dyn ParentWindow>);

impl Window {
    #[cfg_attr(
        not(all(
            feature = "create-native",
            not(any(target_os = "ios", target_os = "android"))
        )),
        allow(dead_code)
    )]
    pub(crate) fn new(window: Arc<dyn ParentWindow>) -> Window {
        Window(window)
    }

    /// The raw window handle; none if the window refuses it.
    pub fn raw_window(&self) -> Option<RawWindowHandle> {
        self.0.window_handle().ok().map(|h| h.as_raw())
    }

    /// The same window as what a file chooser is attached to
    /// (`files::set_parent_window` takes it).
    pub fn as_parent(&self) -> Arc<dyn ParentWindow> {
        self.0.clone()
    }
}

impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Window").field(&self.raw_window()).finish()
    }
}

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

/// The native window showing `viewport`, reached by its id: the main
/// window ([`ViewportId::ROOT`]) as soon as eframe created it, on every
/// system; another deferred viewport's once it was painted for a redraw
/// request and the window ids were verified ([`ids_verified`]). None for
/// an immediate viewport (painted inside its parent's window), a viewport
/// not open (or closed since), when called off the event loop's thread,
/// or when egui-pop does not own the event loop.
pub fn window(viewport: ViewportId) -> Option<Window> {
    native::window(viewport)
}

/// The viewports whose native window is known ([`window`]), the main
/// window first.
pub fn windows() -> Vec<ViewportId> {
    native::windows()
}

/// The screens, in points (each monitor's physical rectangle over its
/// scale factor): those told to egui-pop (`window::screens`), by this
/// runner before the main window is created and again when they change,
/// or by a host running the event loop itself (`window::set_screens`);
/// empty until told.
pub fn screens() -> Vec<Rect> {
    crate::window::screens()
}

/// The raw handle of the window showing `viewport` ([`window`]): the main
/// window's as eframe gives it, another window's once it was painted for
/// a redraw request (eframe paints a window still hidden at start
/// outside of one) and the window ids were verified ([`ids_verified`]);
/// none for an immediate viewport, or when egui-pop does not own the
/// event loop.
pub fn window_handle(viewport: ViewportId) -> Option<RawWindowHandle> {
    native::window_handle(viewport)
}

/// Whether winit's window ids were seen to be the native windows: the
/// main window's id read as a handle, compared with the handle eframe
/// gives for it. `None` until the main window is painted, or when
/// egui-pop does not own the event loop; `Some(false)` on a system where
/// they differ (macOS, or a winit that changed), where no window's
/// handles are recorded beyond the main window's.
pub fn ids_verified() -> Option<bool> {
    native::ids_verified()
}

/// `eframe::run_native`, or eframe on egui-pop's own event loop.
pub(crate) fn run(
    title: &str,
    options: eframe::NativeOptions,
    creator: eframe::AppCreator<'_>,
) -> eframe::Result {
    native::run(title, options, creator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{Screen, repair_place};

    #[test]
    fn no_window_before_the_loop_runs() {
        assert!(window(ViewportId::ROOT).is_none());
        assert!(window(ViewportId::from_hash_of("secondary")).is_none());
        assert!(windows().is_empty());
        assert!(window_handle(ViewportId::ROOT).is_none());
    }

    #[test]
    fn a_monitor_in_points() {
        let s = Screen::from_physical(Some("DP-1".into()), (2560, 0), (2560, 1440), 2.0);
        assert_eq!(
            s.rect,
            Rect::from_min_size(egui::pos2(1280.0, 0.0), egui::vec2(1280.0, 720.0))
        );
        assert_eq!(s.name.as_deref(), Some("DP-1"));
        assert_eq!(s.scale, 2.0);
        let s = Screen::from_physical(None, (0, 0), (800, 600), 0.0);
        assert_eq!(s.rect.size(), egui::vec2(800.0, 600.0));
    }

    #[test]
    fn a_place_off_every_screen_comes_back() {
        let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 800.0));
        let vb = egui::ViewportBuilder::default()
            .with_position([5000.0, 4000.0])
            .with_inner_size([800.0, 600.0]);
        let vb = repair_place(vb, &[screen]);
        let p = vb.position.expect("a position");
        assert!(screen.contains(p), "{p:?} is on the screen");
        assert_eq!(vb.inner_size, Some(egui::vec2(800.0, 600.0)));
    }

    #[test]
    fn a_size_larger_than_its_screen_shrinks() {
        let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 800.0));
        let vb = egui::ViewportBuilder::default()
            .with_position([10.0, 10.0])
            .with_inner_size([3000.0, 2000.0]);
        let vb = repair_place(vb, &[screen]);
        assert_eq!(vb.inner_size, Some(egui::vec2(1280.0, 800.0)));
        assert_eq!(vb.position, Some(egui::pos2(10.0, 10.0)));
    }

    #[test]
    fn no_screens_no_change() {
        let vb = egui::ViewportBuilder::default()
            .with_position([5000.0, 4000.0])
            .with_inner_size([800.0, 600.0]);
        let vb = repair_place(vb, &[]);
        assert_eq!(vb.position, Some(egui::pos2(5000.0, 4000.0)));
    }
}
