//! The main window brought to the front, as the platform services do
//! when a second launch, a tray click or a notification asks for the
//! application. In an application it is the bus's `pop.window.activate` of
//! the main window ([`raise_app`]): the registry holds it as the active
//! window and its `pop.window.raise` listener brings it forward.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>

use egui::Context;
use egui_pop_api::AppHandle;
use egui_pop_api::bus::{Reason, WindowKind, WindowRequest};
use egui_pop_api::windows::WindowId;

/// Who asks the main window to the front: the platform services.
pub const OWNER: &str = "egui_pop.platform";

/// Asks the application's main window to the front: its
/// `pop.window.activate` with the reason `command`, owned by [`OWNER`]. The
/// window is shown, restored when minimized and asked for the focus. On
/// Wayland a window cannot take the focus by itself (the compositor needs
/// an activation token, which egui does not pass on): the window asks
/// for attention instead, and the desktop says so.
pub fn raise_app(app: &AppHandle) {
    app.request_window_activate(WindowRequest::new(
        WindowId::MAIN,
        WindowKind::Main,
        OWNER,
        Reason::Command,
    ));
}

/// The same for a program without an egui-pop application (the platform
/// readers used standalone): the main window shown, restored and asked
/// for the focus, attention asked on Wayland. An application uses
/// [`raise_app`], so the bus knows the active window.
pub fn raise(ctx: &Context) {
    egui_pop_api::bus::raise_main_window(ctx);
}

/// Whether the session is a Wayland one.
pub fn wayland() -> bool {
    cfg!(all(unix, not(target_os = "macos")))
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland"))
}
