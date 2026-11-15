//! The one place that borrows raw handles (`unsafe`).

#![allow(unsafe_code)]

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WindowHandle,
};

/// A window's raw handles, read from its winit id ([`crate::Display::window`]).
///
/// They name a window winit keeps: valid while it lives. Keep them no
/// longer than that (drop them when winit reports the window destroyed);
/// as with the handles winit gives, use them on the thread that runs the
/// event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowHandles {
    window: RawWindowHandle,
    display: RawDisplayHandle,
}

impl WindowHandles {
    pub(crate) fn new(window: RawWindowHandle, display: RawDisplayHandle) -> WindowHandles {
        WindowHandles { window, display }
    }

    /// The raw window handle.
    pub fn raw_window(&self) -> RawWindowHandle {
        self.window
    }
}

// SAFETY: plain identifiers of a window and its display (an `HWND`, an X11
// id, `wl_surface` and `wl_display` pointers); this type never
// dereferences them. Whoever uses them follows the platform's threading
// rules, as with the handles winit's `Window` gives (which is `Send` and
// `Sync` too).
unsafe impl Send for WindowHandles {}
// SAFETY: as above.
unsafe impl Sync for WindowHandles {}

impl HasWindowHandle for WindowHandles {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: the handle names a window winit keeps alive; the type's
        // contract is to be dropped once winit destroys it.
        Ok(unsafe { WindowHandle::borrow_raw(self.window) })
    }
}

impl HasDisplayHandle for WindowHandles {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        // SAFETY: the display was read from the running event loop, which
        // outlives each of its windows.
        Ok(unsafe { DisplayHandle::borrow_raw(self.display) })
    }
}
