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
//!   window; eframe calls the window's `window_builder` hook inside it, so
//!   the place restored from the last run is repaired against real screens
//!   (`WindowState::repair`), not only bounded. [`screens`] gives them.
//! - **Every window's handles.** eframe hands out the main window's only.
//!   It paints a window synchronously in that window's `RedrawRequested`,
//!   so the handler notes which window is being painted, and a
//!   `Context::on_begin_pass` hook pairs it with the pass's viewport. The
//!   handles come from the window's winit id (`egui_pop_winit`, where the
//!   `unsafe` lives), checked first against the main window's handles from
//!   eframe: they are used only once both agree ([`ids_verified`]). Each
//!   window's handles are recorded where egui-pop keeps them per viewport
//!   (`files::set_parent_window`, `frame::set_native_window`) and forgotten
//!   when winit destroys the window. [`window_handle`] gives them.
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
    use egui::{Rect, ViewportId};
    use raw_window_handle::RawWindowHandle;

    pub(super) fn screens() -> Vec<Rect> {
        Vec::new()
    }

    pub(super) fn window_handle(_: ViewportId) -> Option<RawWindowHandle> {
        None
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

use egui::{Rect, ViewportId};
use raw_window_handle::RawWindowHandle;

/// The screens the event loop saw before the main window was created, in
/// points (each monitor's physical rectangle over its scale factor); empty
/// before the loop runs, or when egui-pop does not own it.
pub fn screens() -> Vec<Rect> {
    native::screens()
}

/// The raw handle of the window showing `viewport`, once it was painted
/// for a redraw request (eframe paints a window still hidden at start
/// outside of one) and the window ids were verified ([`ids_verified`]); none for an
/// immediate viewport, or when egui-pop does not own the event loop.
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

/// The monitors, in points: each one's physical rectangle over its own
/// scale factor.
#[cfg_attr(
    not(all(
        feature = "create-native",
        not(any(target_os = "ios", target_os = "android"))
    )),
    allow(dead_code)
)]
pub(crate) fn screen_rect(position: (i32, i32), size: (u32, u32), scale: f64) -> Rect {
    let s = if scale > 0.0 { scale as f32 } else { 1.0 };
    Rect::from_min_size(
        egui::pos2(position.0 as f32 / s, position.1 as f32 / s),
        egui::vec2(size.0 as f32 / s, size.1 as f32 / s),
    )
}

/// The main window's builder, its restored place repaired against
/// `screens`: a position off every screen is pulled back onto the first,
/// a size larger than its screen shrunk. Maximized and full-screen flags
/// are kept as they are.
#[cfg_attr(
    not(all(
        feature = "create-native",
        not(any(target_os = "ios", target_os = "android"))
    )),
    allow(dead_code)
)]
pub(crate) fn repair_place(
    mut vb: egui::ViewportBuilder,
    screens: &[Rect],
) -> egui::ViewportBuilder {
    if screens.is_empty() {
        return vb;
    }
    let Some(size) = vb.inner_size else {
        return vb;
    };
    let state = crate::window::WindowState {
        position: vb.position,
        size,
        maximized: false,
        fullscreen: false,
    }
    .repair(screens);
    vb.position = state.position;
    vb.inner_size = Some(state.size);
    vb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_monitor_in_points() {
        let r = screen_rect((2560, 0), (2560, 1440), 2.0);
        assert_eq!(
            r,
            Rect::from_min_size(egui::pos2(1280.0, 0.0), egui::vec2(1280.0, 720.0))
        );
        let r = screen_rect((0, 0), (800, 600), 0.0);
        assert_eq!(r.size(), egui::vec2(800.0, 600.0));
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
