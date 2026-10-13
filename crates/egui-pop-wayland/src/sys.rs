//! The one place that touches raw Wayland pointers: joining winit's
//! connection, naming winit's surface, and handing wgpu a handle to a
//! surface of ours.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::fmt;
use std::ptr::NonNull;
use std::sync::Arc;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy};

/// A connection on winit's `wl_display`, in guest mode: it never closes
/// the display.
///
/// The display must outlive the connection: it is the window's event
/// loop's, which eframe drops after the application and its renderer,
/// where egui-pop keeps the connection.
pub(crate) fn connection(display: NonNull<c_void>) -> Connection {
    // SAFETY: `display` is the live `wl_display` of the window's handle;
    // the event loop owning it outlives the egui context and renderer
    // that hold this connection (see above).
    let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
    Connection::from_backend(backend)
}

/// winit's `wl_surface`, as a proxy on `conn` (used only as an argument:
/// its events stay winit's).
pub(crate) fn foreign_surface(
    conn: &Connection,
    surface: NonNull<c_void>,
) -> Result<WlSurface, String> {
    // SAFETY: `surface` is the live `wl_surface` of the window's handle,
    // on the display `conn` joined; it is used while the window lives
    // (to make subsurfaces and the preferred-description feedback).
    let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.as_ptr().cast()) }
        .map_err(|_| "the window's surface is not a wl_surface".to_string())?;
    WlSurface::from_id(conn, id).map_err(|_| "the window's surface is not alive".to_string())
}

/// A surface of ours and its connection, destroyed when the last holder
/// goes (the subsurface, or the wgpu surface made on its handle).
pub(crate) struct Keep {
    pub(crate) surface: WlSurface,
    pub(crate) conn: Connection,
}

impl Drop for Keep {
    fn drop(&mut self) {
        self.surface.destroy();
        let _ = self.conn.flush();
    }
}

/// The raw handles of an image surface, for wgpu
/// (`Instance::create_surface`): the window's display and the
/// subsurface's `wl_surface`, kept alive as long as the handle is.
#[derive(Clone)]
pub struct SurfaceHandle {
    pub(crate) keep: Arc<Keep>,
}

impl fmt::Debug for SurfaceHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SurfaceHandle")
            .field("surface", &self.keep.surface.id())
            .finish()
    }
}

impl HasWindowHandle for SurfaceHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let surface =
            NonNull::new(self.keep.surface.id().as_ptr().cast()).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface));
        // SAFETY: the `wl_surface` is ours and is destroyed only when the
        // last clone of this handle (and of its subsurface) is dropped.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for SurfaceHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let display = NonNull::new(self.keep.conn.backend().display_ptr().cast())
            .ok_or(HandleError::Unavailable)?;
        let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display));
        // SAFETY: the display is winit's, alive as long as the connection
        // this handle keeps (see `connection`).
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}
