//! The winit layer of egui-pop's runner.
//!
//! eframe creates and keeps every window of an application, and hands out
//! the main one only (`CreationContext::window_handle`): a file chooser
//! opened from a secondary window, a hit test or a surface on one has no
//! handle to take (see the register of upstream workarounds,
//! `docs/upstream.md`). egui-pop runs eframe on an event loop it owns
//! (`eframe::create_native`), so it sees each window's id; on the desktop
//! systems winit 0.30 makes that id the native window itself:
//!
//! | Windowing system | `WindowId` holds | Raw handle |
//! |---|---|---|
//! | Windows | the `HWND` | [`RawWindowHandle::Win32`] |
//! | X11 (Xlib or XCB) | the X11 window id | [`RawWindowHandle::Xlib`], [`RawWindowHandle::Xcb`] |
//! | Wayland | the `wl_surface` proxy | [`RawWindowHandle::Wayland`] |
//! | macOS | the `NSWindow` (the handle wants its `NSView`) | none |
//!
//! That is an implementation detail of winit, not its API: the crate's
//! `window_id` test checks it on each system against the handle winit
//! gives a window ([`same_window`]: the window, not the extras winit adds), and egui-pop's runner checks it at run time against
//! the main window's, using the ids only when they agree.
//!
//! The crate knows nothing of egui (it sits under every egui-pop layer).
//! [`Display::of`] reads the event loop's display; [`Display::window`]
//! turns a window id into [`WindowHandles`], which implement
//! [`HasWindowHandle`](raw_window_handle::HasWindowHandle) and
//! [`HasDisplayHandle`] for whatever takes a
//! window (rfd's parent, a Wayland subsurface). What a window id maps to
//! is decided by [`raw_window`], plain code tested on every system; the
//! `sys` module only borrows the handles.

use raw_window_handle::{
    HasDisplayHandle, RawDisplayHandle, RawWindowHandle, WaylandWindowHandle, Win32WindowHandle,
    XcbWindowHandle, XlibWindowHandle,
};
use std::ffi::c_void;
use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

mod sys;

pub use sys::WindowHandles;

/// The display of an event loop: what its window ids are read against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Display(RawDisplayHandle);

impl Display {
    /// The display `event_loop` runs on; none when winit gives no handle.
    pub fn of(event_loop: &ActiveEventLoop) -> Option<Display> {
        event_loop
            .display_handle()
            .ok()
            .map(|d| Display(d.as_raw()))
    }

    /// A display from its raw handle (a host that has no
    /// `ActiveEventLoop` at hand, a test).
    pub fn from_raw(raw: RawDisplayHandle) -> Display {
        Display(raw)
    }

    /// The raw display handle.
    pub fn raw(&self) -> RawDisplayHandle {
        self.0
    }

    /// The raw window handle of the window `id` names, on this display;
    /// none where the id is not the native window (macOS, other systems).
    pub fn window_handle(&self, id: WindowId) -> Option<RawWindowHandle> {
        raw_window(u64::from(id), self.0)
    }

    /// The window's handles, for what takes `HasWindowHandle` and
    /// `HasDisplayHandle`. They stay valid while the window lives: whoever
    /// keeps them drops them when winit reports it destroyed.
    pub fn window(&self, id: WindowId) -> Option<WindowHandles> {
        let window = self.window_handle(id)?;
        Some(WindowHandles::new(window, self.0))
    }
}

/// What a winit 0.30 window id (`u64::from(id)`) is on a display: the
/// native window it names, per windowing system. `None` on macOS (the id
/// is the `NSWindow`, the handle wants its `NSView`), for an id that
/// cannot be a window (zero, out of range), and on other systems.
pub fn raw_window(id: u64, display: RawDisplayHandle) -> Option<RawWindowHandle> {
    match display {
        RawDisplayHandle::Windows(_) => NonZeroIsize::new(isize::try_from(id).ok()?)
            .map(|h| RawWindowHandle::Win32(Win32WindowHandle::new(h))),
        RawDisplayHandle::Xlib(_) => {
            let w = std::ffi::c_ulong::try_from(id).ok()?;
            (w != 0).then(|| RawWindowHandle::Xlib(XlibWindowHandle::new(w)))
        }
        RawDisplayHandle::Xcb(_) => NonZeroU32::new(u32::try_from(id).ok()?)
            .map(|w| RawWindowHandle::Xcb(XcbWindowHandle::new(w))),
        RawDisplayHandle::Wayland(_) => NonNull::new(usize::try_from(id).ok()? as *mut c_void)
            .map(|p| RawWindowHandle::Wayland(WaylandWindowHandle::new(p))),
        _ => None,
    }
}

/// Whether two raw handles name the same window: the window itself (the
/// `HWND`, the X11 id, the `wl_surface`), not the extras winit fills in
/// (the `HINSTANCE`, the X11 visual) and a handle read from an id lacks.
pub fn same_window(a: &RawWindowHandle, b: &RawWindowHandle) -> bool {
    match (a, b) {
        (RawWindowHandle::Win32(a), RawWindowHandle::Win32(b)) => a.hwnd == b.hwnd,
        (RawWindowHandle::Xlib(a), RawWindowHandle::Xlib(b)) => a.window == b.window,
        (RawWindowHandle::Xcb(a), RawWindowHandle::Xcb(b)) => a.window == b.window,
        (RawWindowHandle::Wayland(a), RawWindowHandle::Wayland(b)) => a.surface == b.surface,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raw_window_handle::{
        AppKitDisplayHandle, WaylandDisplayHandle, WindowsDisplayHandle, XcbDisplayHandle,
        XlibDisplayHandle,
    };

    #[test]
    fn an_id_is_the_native_window_per_system() {
        let win = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
        assert!(matches!(
            raw_window(0x1234, win),
            Some(RawWindowHandle::Win32(h)) if h.hwnd.get() == 0x1234
        ));
        let xlib = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));
        assert!(matches!(
            raw_window(0x40_0003, xlib),
            Some(RawWindowHandle::Xlib(h)) if h.window == 0x40_0003
        ));
        let xcb = RawDisplayHandle::Xcb(XcbDisplayHandle::new(None, 0));
        assert!(matches!(
            raw_window(0x40_0003, xcb),
            Some(RawWindowHandle::Xcb(h)) if h.window.get() == 0x40_0003
        ));
        let mut display = 0u8;
        let wl = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new((&raw mut display).cast()).unwrap(),
        ));
        assert!(matches!(
            raw_window(0x5555_0000, wl),
            Some(RawWindowHandle::Wayland(h)) if h.surface.as_ptr() as usize == 0x5555_0000
        ));
    }

    #[test]
    fn the_same_window_without_its_extras() {
        let mut from_winit = XlibWindowHandle::new(0x40_0003);
        from_winit.visual_id = 33;
        let xlib = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));
        let from_id = raw_window(0x40_0003, xlib).unwrap();
        assert_ne!(from_id, RawWindowHandle::Xlib(from_winit));
        assert!(same_window(&from_id, &RawWindowHandle::Xlib(from_winit)));
        let other = raw_window(0x40_0004, xlib).unwrap();
        assert!(!same_window(&from_id, &other));
        let mut win = Win32WindowHandle::new(NonZeroIsize::new(0x1234).unwrap());
        win.hinstance = NonZeroIsize::new(0x40_0000);
        let windows = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
        assert!(same_window(
            &raw_window(0x1234, windows).unwrap(),
            &RawWindowHandle::Win32(win)
        ));
    }

    #[test]
    fn no_handle_where_the_id_is_not_one() {
        let xcb = RawDisplayHandle::Xcb(XcbDisplayHandle::new(None, 0));
        assert_eq!(raw_window(0, xcb), None);
        assert_eq!(raw_window(u64::MAX, xcb), None);
        let win = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
        assert_eq!(raw_window(0, win), None);
        let mac = RawDisplayHandle::AppKit(AppKitDisplayHandle::new());
        assert_eq!(raw_window(0x1234, mac), None);
    }
}
