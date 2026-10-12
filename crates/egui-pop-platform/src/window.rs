//! The main window brought to the front, as the platform services do
//! when a second launch, a tray click or a notification asks for the
//! application.

use egui::{Context, UserAttentionType, ViewportCommand};

/// Shows the main window, restores it when minimized and asks for the
/// focus. On Wayland a window cannot take the focus by itself (the
/// compositor needs an activation token, which egui does not pass on):
/// the window asks for attention instead, and the desktop says so.
pub fn raise(ctx: &Context) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    if wayland() {
        ctx.send_viewport_cmd(ViewportCommand::RequestUserAttention(
            UserAttentionType::Informational,
        ));
    }
    ctx.request_repaint();
}

/// Whether the session is a Wayland one.
pub fn wayland() -> bool {
    cfg!(all(unix, not(target_os = "macos")))
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland"))
}
