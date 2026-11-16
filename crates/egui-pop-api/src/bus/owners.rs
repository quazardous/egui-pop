//! A window's owner on the bus: the `window.open` an owner asks, and the
//! windows a plugin leaves when it goes (ADR 0029, slice 3).

use bus_contract::{Bus, ListenOpts};
use serde_json::json;

use super::{OWNER, PluginDisconnect, Reason, WindowKind, WindowOpen, WindowRequest, asking, emit};
use crate::app::AppHandle;

/// Closes the application windows a disconnected plugin owned, before
/// `plugin.disconnected` is said: each a `window.close` (`owner-gone`),
/// counted as `windows_closed`.
pub const OWNER_GONE: &str = "window.owner-gone";

/// Registers egui-pop's listeners of a window's owner.
pub(super) fn install(kernel: &dyn Bus) {
    let listened = kernel.listen::<PluginDisconnect>(
        OWNER_GONE,
        ListenOpts::new().owner(OWNER),
        |change, counts| {
            let Some(app) = asking() else { return Ok(()) };
            let left: Vec<WindowRequest> = app
                .windows_list()
                .into_iter()
                .filter(|w| w.kind == WindowKind::App)
                .filter(|w| w.owner == change.plugin || w.owner == change.owner)
                .map(|w| WindowRequest::new(w.id, w.kind, w.owner, Reason::OwnerGone))
                .collect();
            let mut closed = 0;
            for req in left {
                if app.request_window_close(req) {
                    closed += 1;
                }
            }
            if closed > 0 {
                counts.insert("windows_closed".into(), json!(closed));
            }
            Ok(())
        },
    );
    debug_assert!(listened.is_ok(), "{listened:?}");
}

impl AppHandle {
    /// Runs `window.open`'s listeners for a window its owner asks: what
    /// they did, one line each.
    pub(crate) fn ask_window_open(&self, req: WindowRequest) -> Vec<String> {
        emit::<WindowOpen>(self, req)
    }
}
