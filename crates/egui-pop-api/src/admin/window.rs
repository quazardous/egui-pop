//! The Settings app's window on the bus (ADR 0029, slice 3): the app is
//! the owner of its application window ([`super::APP_WINDOW`]) and asks
//! for it, `window.open` and `window.close`, whenever what it wants
//! changes (opened, closed, moved between the overlay and its window); its
//! listener (`admin.window`) keeps the app's state with the window,
//! whoever asked. The registry ([`AppHandle::windows_list`]) is the one
//! truth for whether the window is there: no flag, nothing followed from
//! a pass.

use std::sync::Arc;

use egui_pop_core::lock;
use egui_pop_hooks::{ListenOpts, Point as _};

use super::{APP_WINDOW, Admin, OWNER, WINDOW_LISTENER, in_window};
use crate::app::AppHandle;
use crate::bus::{self, CHILDREN, FOCUS, REGISTRY, Reason, WindowClose, WindowOpen};
use crate::windows::WindowId;

/// Whether the registry holds the app's window.
pub(crate) fn held(app: &AppHandle) -> bool {
    app.windows_list().iter().any(|w| w.id == APP_WINDOW)
}

/// Whether the app wants its window: open, in the window mode, where the
/// backend has windows of its own.
fn wanted(app: &AppHandle, admin: &Admin) -> bool {
    admin.is_open() && in_window(app) && !app.egui().embed_viewports()
}

/// Asks the bus for the window the app wants now: `window.open` when it
/// wants one not there (or, `raise`, one there: brought to the front),
/// `window.close` when one is there it no longer wants (moved over the
/// main window: it stays open). Called where what it wants changes, in
/// the same call.
pub(super) fn settle(app: &AppHandle, admin: &Admin, raise: bool) {
    let there = held(app);
    if wanted(app, admin) {
        if !there || raise {
            let spec = super::shell::window_spec(&app.egui());
            app.request_window_open(spec, OWNER, Reason::Command, super::shell::window_content);
        }
    } else if there {
        close(app);
    }
}

/// Asks `window.close` of the app's window, the app asking.
pub(super) fn close(app: &AppHandle) -> bool {
    app.close_entry(&WindowId::new(APP_WINDOW), Some(OWNER), Reason::Command)
}

/// `admin.window`: the Settings app's listener on `window.open` (first:
/// the app is open when its window is) and `window.close` (after the
/// window's children, before the focus moves and the registry lets the
/// window go: the app closes in the same call, whoever asked, unless it
/// is moving over the main window).
pub(super) fn listen(app: &AppHandle, admin: Arc<Admin>) {
    let bus = app.bus();
    // Connected again: its listeners are replaced.
    bus.unlisten(WindowOpen::ID, WINDOW_LISTENER);
    bus.unlisten(WindowClose::ID, WINDOW_LISTENER);
    let opened = {
        let admin = admin.clone();
        bus.listen::<WindowOpen>(
            WINDOW_LISTENER,
            ListenOpts::new().owner(OWNER).before(REGISTRY),
            move |req, did| {
                if req.window == APP_WINDOW {
                    lock(&admin.state).open = true;
                    did.push(format!(
                        "{WINDOW_LISTENER}: the Settings app is open in its window"
                    ));
                }
                Ok(())
            },
        )
    };
    let closed = bus.listen::<WindowClose>(
        WINDOW_LISTENER,
        ListenOpts::new().owner(OWNER).after(CHILDREN).before(FOCUS),
        move |req, did| {
            if req.window != APP_WINDOW || !admin.is_open() {
                return Ok(());
            }
            // Moving over the main window ("Pop in", a focus screen): the
            // app stays open there.
            let moving = bus::asking().is_some_and(|app| !in_window(&app));
            if moving {
                did.push(format!(
                    "{WINDOW_LISTENER}: the Settings app moves over the main window"
                ));
            } else {
                admin.close_state();
                did.push(format!("{WINDOW_LISTENER}: the Settings app closed"));
            }
            Ok(())
        },
    );
    for l in [opened, closed] {
        if let Err(e) = l {
            log::error!(target: "egui_pop", "{e}");
        }
    }
}
