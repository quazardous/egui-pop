//! The active window on the bus (`window.activate`, ADR 0029): the window
//! the person works in, one truth held by the registry.
//!
//! A press, a key or text in a window asks it (`user`), a window gaining
//! the system's focus asks it (`system`), the application asking a window
//! to the front asks it (`command`). `window.registry` holds it as the
//! active window; [`RAISE`] shows, restores and focuses a window asked by
//! a command, the one place egui-pop sends `ViewportCommand::Focus`;
//! `window.activated` is said after the last listener when the active
//! window changed. A window closing while active hands the activation to
//! the window worked in before it (`window.focus`, a `system` request).

use std::sync::{Arc, Mutex};

use bus_contract::{Bus, ListenOpts, Point as _};
use egui::{Context, UserAttentionType, ViewportCommand, ViewportId};
use egui_pop_core::lock;
use serde_json::json;

use super::{
    OWNER, RAISE, REGISTRY, Reason, Registry, WindowActivate, WindowKind, WindowRequest, asking,
    emit,
};
use crate::app::AppHandle;
use crate::windows::WindowId;

/// Registers `window.activate`'s listeners: the registry, then the raise
/// (the invariants, last, are the bus's).
pub(super) fn install(kernel: &dyn Bus, registry: &Arc<Mutex<Registry>>) {
    let mine = || ListenOpts::new().owner(OWNER);
    let registry = registry.clone();
    let listened = [
        kernel.listen::<WindowActivate>(REGISTRY, mine().before(RAISE), move |req, did| {
            let mut registry = lock(&registry);
            if registry.record(&req.window).is_none() {
                return Err(format!("`{}` is not in the registry", req.window).into());
            }
            registry.activate(&req.window);
            did.push(format!("{REGISTRY}: `{}` is active", req.window));
            Ok(())
        }),
        kernel.listen::<WindowActivate>(RAISE, mine().after(REGISTRY), |req, did| {
            if req.reason != Reason::Command {
                return Ok(());
            }
            let Some(app) = asking() else { return Ok(()) };
            let Some(v) = crate::window_invariants::viewport_of(req.kind, &req.window) else {
                return Ok(());
            };
            raise(&app.egui(), v, req.kind == WindowKind::Main);
            if req.kind == WindowKind::App {
                app.0.bus.raise_pending.borrow_mut().insert(v);
            }
            did.push(format!("{RAISE}: `{}` to the front", req.window));
            Ok(())
        }),
    ];
    for l in listened {
        debug_assert!(l.is_ok(), "{l:?}");
    }
}

/// Shows a window, restores it when minimized and asks the system's focus
/// for it. On Wayland a window cannot take the focus by itself (the
/// compositor needs an activation token, which egui does not pass on):
/// the main window asks for attention instead, and the desktop says so.
fn raise(ctx: &Context, v: ViewportId, main: bool) {
    ctx.send_viewport_cmd_to(v, ViewportCommand::Visible(true));
    ctx.send_viewport_cmd_to(v, ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd_to(v, ViewportCommand::Focus);
    if main && wayland() {
        ctx.send_viewport_cmd_to(
            v,
            ViewportCommand::RequestUserAttention(UserAttentionType::Informational),
        );
    }
    ctx.request_repaint_of(v);
}

/// The main window shown, restored and focused, for a program without an
/// egui-pop application (the platform readers used standalone): what
/// [`RAISE`] does for a `window.activate` of the main window. An
/// application asks `window.activate` instead
/// ([`AppHandle::request_window_activate`]).
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn raise_main_window(ctx: &Context) {
    raise(ctx, ViewportId::ROOT, true);
}

/// Whether the session is a Wayland one.
fn wayland() -> bool {
    cfg!(all(unix, not(target_os = "macos")))
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland"))
}

impl AppHandle {
    /// Asks for a window to become the active one, the window worked in
    /// (`window.activate`, ADR 0029): the registry holds it as active, and
    /// with the reason `command` it is brought to the front. Its listeners
    /// run now, in their declared order; `window.activated` is said after
    /// the last one when the active window changed. A window not in the
    /// registry is refused, said as `window.refused`. Whether it ran.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_window_activate(&self, request: WindowRequest) -> bool {
        let (before, held) = {
            let r = lock(&self.0.bus.registry);
            (r.active().0, r.record(&request.window).map(|w| w.kind))
        };
        let Some(kind) = held else {
            self.publish(
                "egui_pop",
                request.cause,
                "window.refused",
                json!({ "request": WindowActivate::ID, "window": request.window, "owner": request.owner, "reason": "no open window of this id" }),
            );
            return false;
        };
        let request = WindowRequest { kind, ..request };
        let did = emit::<WindowActivate>(self, request.clone());
        let after = self.active_window_id();
        if after != before {
            // The other windows lost the focus, whether or not they drew a
            // pass since: one focused again gains it anew.
            let here = crate::window_invariants::viewport_of(request.kind, &request.window);
            self.0
                .bus
                .focus_seen
                .borrow_mut()
                .retain(|v, _| Some(*v) == here);
            self.publish(
                "egui_pop",
                request.cause,
                "window.activated",
                json!({
                    "window": request.window,
                    "kind": request.kind,
                    "owner": request.owner,
                    "reason": request.reason,
                    "from": before,
                    "did": did,
                }),
            );
        }
        true
    }

    /// The active window's id in the registry (`main`, an application
    /// window's id, `panel:<id>`).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn active_window_id(&self) -> String {
        lock(&self.0.bus.registry).active().0
    }

    /// The active window as the registry holds it: its id and kind.
    pub(crate) fn active_window_record(&self) -> (String, WindowKind) {
        lock(&self.0.bus.registry).active()
    }

    /// The window of the registry a viewport draws (the main window, a
    /// detached panel's, an application window's): its id and kind.
    pub(crate) fn registry_window_at(&self, v: ViewportId) -> Option<(String, WindowKind)> {
        let shown = format!("{v:?}");
        lock(&self.0.bus.registry)
            .records
            .iter()
            .find(|r| {
                matches!(
                    r.kind,
                    WindowKind::Main | WindowKind::App | WindowKind::Panel
                ) && r.viewport.as_deref() == Some(shown.as_str())
            })
            .map(|r| (r.id.clone(), r.kind))
    }

    /// Asks a window to the front (`window.activate`, reason `command`):
    /// the window a viewport draws.
    pub(crate) fn raise_viewport(&self, v: ViewportId, owner: &str) {
        if let Some((id, kind)) = self.registry_window_at(v) {
            self.request_window_activate(WindowRequest::new(id, kind, owner, Reason::Command));
        }
    }

    /// A window blocked by a modal open in another of its group
    /// ([`egui_pop_core::overlay::block_window_raising`]): a click on its
    /// backdrop asks the modal's window to the front (`window.activate`).
    pub(crate) fn block_window(
        &self,
        ctx: &Context,
        same: &dyn Fn(ViewportId) -> bool,
    ) -> Option<ViewportId> {
        egui_pop_core::overlay::block_window_raising(ctx, same, &|owner| {
            self.raise_viewport(owner, "egui_pop.dialogs");
        })
    }

    /// The window being drawn asks to be the active one when the person
    /// pressed, typed or wrote in it this pass (`user`), or when it gained
    /// the system's focus (`system`). Nothing is asked for the window
    /// already active.
    pub(crate) fn note_window(&self, ctx: &Context) {
        let v = ctx.viewport_id();
        let (focused, used) = ctx.input(|i| {
            (
                i.viewport().focused == Some(true),
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key { pressed: true, .. }
                            | egui::Event::PointerButton { pressed: true, .. }
                            | egui::Event::Text(_)
                    )
                }),
            )
        });
        let had = self.0.bus.focus_seen.borrow_mut().insert(v, focused);
        let gained = focused && had != Some(true);
        if !used && !gained {
            return;
        }
        let Some((id, kind)) = self.registry_window_at(v) else {
            return;
        };
        if self.active_window_id() == id {
            return;
        }
        let reason = if used { Reason::User } else { Reason::System };
        self.request_window_activate(WindowRequest::new(id, kind, OWNER, reason));
    }

    /// From an application window's own pass: the focus asked again when
    /// a command raised it since (`window.raise`), so a window not yet
    /// drawn when it was raised gets it too.
    pub(crate) fn raise_in_pass(&self, ctx: &Context) {
        if self
            .0
            .bus
            .raise_pending
            .borrow_mut()
            .remove(&ctx.viewport_id())
        {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    /// A closing window's focus is forgotten: a window of the same id
    /// opened again and focused gains the focus anew.
    pub(crate) fn forget_focus(&self, req: &WindowRequest) {
        if let Some(v) = crate::window_invariants::viewport_of(req.kind, &req.window) {
            self.0.bus.focus_seen.borrow_mut().remove(&v);
            self.0.bus.raise_pending.borrow_mut().remove(&v);
        }
    }

    /// `window.focus`'s part on a window closing: when it is the active
    /// one, the window worked in before it becomes active (a `system`
    /// request). The window made active, if one was.
    pub(crate) fn focus_before_close(&self, closing: &str) -> Option<String> {
        if closing == WindowId::MAIN {
            return None;
        }
        let (id, kind) = {
            let r = lock(&self.0.bus.registry);
            if r.active().0 != closing {
                return None;
            }
            r.by_recency()
                .find(|w| {
                    w.id != closing
                        && matches!(
                            w.kind,
                            WindowKind::Main | WindowKind::App | WindowKind::Panel
                        )
                })
                .map(|w| (w.id.clone(), w.kind))
                .unwrap_or_else(|| (WindowId::MAIN.to_string(), WindowKind::Main))
        };
        self.request_window_activate(WindowRequest::new(id.clone(), kind, OWNER, Reason::System));
        Some(id)
    }

    /// The application windows other than the main one, worked in last
    /// first (those never activated after, in opening order).
    pub(crate) fn app_windows_by_recency(&self) -> Vec<String> {
        let r = lock(&self.0.bus.registry);
        let mut out: Vec<String> = r
            .by_recency()
            .filter(|w| w.kind == WindowKind::App)
            .map(|w| w.id.clone())
            .collect();
        for w in r.records.iter().filter(|w| w.kind == WindowKind::App) {
            if !out.contains(&w.id) {
                out.push(w.id.clone());
            }
        }
        out
    }
}
