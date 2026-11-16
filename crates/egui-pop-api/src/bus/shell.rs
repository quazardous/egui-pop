//! The shell's windows on the bus (ADR 0029, slice 4): the main window's
//! close, quitting, and a window's content moving into the main window and
//! back (`window.reparent`).
//!
//! - The main window's close, whatever asked it (the system's close
//!   request, its title bar's Close, [`AppHandle::close_window`],
//!   [`AppHandle::quit`]), is one `window.close` of the main window. Its
//!   step may be vetoed by a before-hook (unsaved work): the close is then
//!   cancelled and nothing changed. With another window open and the
//!   application not quitting, the window worked in last moves into the
//!   main window instead (ADR 0025): a `window.reparent`, and the close is
//!   cancelled.
//! - Closing the main window closes every other window first, each by its
//!   own `window.close` (`window.children`; reason `quit` or
//!   `parent-closed`), so each owner's state closes with its window.
//! - `window.reparent` runs `window.registry` (the content moves; the
//!   registry holds the window or lets it go), `window.place` (each id
//!   keeps its own place), `window.focus` (the main window is the one
//!   worked in), then the invariants.
//!
//! A detached panel's window docked back is a `window.reparent` too (the
//! workbench's listener moves its panels first, [`super::panels`]); while
//! a window's content holds the main window, the detached panels' windows
//! leave the registry with the shell, and come back with it.
//!
//! The system window's own close request stays the way a window's title
//! bar asks it where its content may refuse ([`crate::windows::WindowUi::keep_open`]):
//! the window's pass then asks `window.close`. The raw viewport commands
//! of these paths are here, and nowhere else in the shell.

use std::sync::{Arc, Mutex};

use egui::{Context, ViewportCommand, ViewportId};
use egui_pop_core::lock;
use egui_pop_hooks::{Kernel, ListenOpts, Point as _};
use serde_json::json;

use super::{
    FOCUS, OWNER, PLACE, REGISTRY, Reason, Registry, WindowKind, WindowReparent, WindowRequest,
    asking, emit,
};
use crate::app::AppHandle;
use crate::windows::WindowId;

/// The `window.reparent` step: the request as steps trace it (before-hooks
/// may veto it).
pub const STEP_WINDOW_REPARENT: egui_pop_model::ids::StepId =
    egui_pop_model::ids::StepId::from_static("window.reparent");

/// The key of a main window's close answered by a move into it.
pub(super) const MOVED_IN: &str = "moved_in";

/// Declares the `window.reparent` step.
pub(super) fn declare(ctx: &mut crate::Ctx<'_>) -> Result<(), crate::ctx::CtxError> {
    ctx.step(
        crate::steps::StepSpec::new(
            STEP_WINDOW_REPARENT,
            "A window's content moves to another window (`window`, `parent`: `main` to move into the main window, none to go back to a window of its own): its listeners on the bus run in their declared order.",
        )
        .args(json!({ "window": "import", "kind": "app", "owner": "app", "reason": "user", "parent": "main" })),
    )?;
    Ok(())
}

/// Registers the listeners of `window.reparent`: the registry, the place,
/// the focus (the invariants, last, are the bus's).
pub(super) fn install(kernel: &Kernel, registry: &Arc<Mutex<Registry>>) {
    let mine = || ListenOpts::new().owner(OWNER);
    let registry = registry.clone();
    let listened = [
        kernel.listen::<WindowReparent>(REGISTRY, mine().before(PLACE), move |req, did| {
            let Some(app) = asking() else { return Ok(()) };
            if req.kind == WindowKind::Panel {
                // A detached panel's window, docked back: the workbench's
                // listener moved its panels; the window goes.
                if req.parent.as_deref() == Some(WindowId::MAIN)
                    && lock(&registry).close(&req.window)
                {
                    super::panels::left(&app, &req.window);
                    let mut w = app.windows_mut();
                    w.moved_events
                        .push(("window.closed", json!({ "window": req.window })));
                    w.moved_events.push((
                        "window.moved",
                        json!({ "window": req.window, "to": WindowId::MAIN }),
                    ));
                    did.push(format!("{REGISTRY}: `{}` docked in the main window", req.window));
                }
                return Ok(());
            }
            let id = WindowId::new(&req.window);
            match req.parent.as_deref() {
                Some(WindowId::MAIN) => {
                    if app.move_into_root(&id) {
                        let mut registry = lock(&registry);
                        registry.close(&req.window);
                        did.push(format!(
                            "{REGISTRY}: `{}` drawn in the main window",
                            req.window
                        ));
                        did.extend(super::panels::follow_shell(&app, &mut registry, true));
                    }
                }
                None => {
                    if app.move_out_of_root() {
                        let mut registry = lock(&registry);
                        registry.open(req, Some(id.deferred_viewport()));
                        did.push(format!(
                            "{REGISTRY}: `{}` in a window of its own",
                            req.window
                        ));
                        did.extend(super::panels::follow_shell(&app, &mut registry, false));
                    }
                }
                Some(other) => {
                    return Err(format!(
                        "`{}` cannot move into `{other}`: only the main window takes another's content",
                        req.window
                    )
                    .into());
                }
            }
            Ok(())
        }),
        kernel.listen::<WindowReparent>(PLACE, mine().after(REGISTRY), |_, did| {
            let Some(app) = asking() else { return Ok(()) };
            if app.keep_moved_places() > 0 {
                did.push(format!("{PLACE}: each keeps its place"));
            }
            Ok(())
        }),
        kernel.listen::<WindowReparent>(FOCUS, mine().after(PLACE), |_, did| {
            let Some(app) = asking() else { return Ok(()) };
            // The content moved in is the one worked in; the main window
            // asked to its front is too.
            app.set_active_window(ViewportId::ROOT);
            did.push(format!("{FOCUS}: the main window is active"));
            Ok(())
        }),
    ];
    for l in listened {
        debug_assert!(l.is_ok(), "{l:?}");
    }
}

/// `window.children`'s part on the main window's close: every other
/// window is closed by its own `window.close` first, its owner's state
/// with it (`quit` when the application quits, else `parent-closed`).
pub(super) fn close_children_of_main(app: &AppHandle, req: &WindowRequest, did: &mut Vec<String>) {
    let reason = if req.reason == Reason::Quit {
        Reason::Quit
    } else {
        Reason::ParentClosed
    };
    let children: Vec<WindowRequest> = app
        .windows_list()
        .into_iter()
        .filter(|w| w.kind == WindowKind::App)
        .map(|w| {
            let mut child = WindowRequest::new(w.id, w.kind, w.owner, reason);
            child.cause = req.cause;
            child.parent(WindowId::MAIN)
        })
        .collect();
    let mut closed = 0;
    for child in children {
        if app.request_window_close(child) {
            closed += 1;
        }
    }
    if closed > 0 {
        did.push(format!("{}: {closed} windows closed", super::CHILDREN));
    }
}

/// The step of the main window's close, when another window moves into
/// it instead (not quitting, one open): the `window.reparent` asked, and
/// what the step returns.
pub(super) fn move_in_instead(app: &AppHandle, req: &WindowRequest) -> Option<serde_json::Value> {
    if req.kind != WindowKind::Main || req.reason == Reason::Quit {
        return None;
    }
    let id = app.window_to_move_in()?;
    let owner = app
        .windows_list()
        .into_iter()
        .find(|w| w.id == id.as_str())
        .map_or_else(|| "app".to_string(), |w| w.owner);
    let mut moving = WindowRequest::new(id.as_str(), WindowKind::App, owner, Reason::User)
        .parent(WindowId::MAIN);
    moving.cause = req.cause;
    let moved = app.request_window_reparent(moving);
    Some(json!({ MOVED_IN: id, "reparented": moved }))
}

impl AppHandle {
    /// Asks for a window's content to move to another window
    /// (`window.reparent`): `parent` `main` moves an application window's
    /// content into the main window (its own window goes); no `parent`
    /// moves the content the main window holds back to a window of its
    /// own. Its listeners all run now, in their declared order; a
    /// before-hook on the `window.reparent` step may veto it
    /// (`window.refused`). Whether it moved.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_window_reparent(&self, request: WindowRequest) -> bool {
        let args = serde_json::to_value(&request).unwrap_or_default();
        let outcome = self.step(&STEP_WINDOW_REPARENT, args, |app, args| {
            let req = serde_json::from_value::<WindowRequest>(args.clone())
                .map_err(|e| format!("not a window request: {e}"))?;
            let did = emit::<WindowReparent>(app, req);
            Ok(json!({ "did": did }))
        });
        let said = std::mem::take(&mut self.windows_mut().moved_events);
        match outcome {
            crate::steps::Outcome::Vetoed { by, reason } => {
                self.publish(
                    "egui_pop",
                    request.cause,
                    "window.refused",
                    json!({ "request": WindowReparent::ID, "window": request.window, "owner": request.owner, "vetoed_by": by, "reason": reason }),
                );
                false
            }
            crate::steps::Outcome::Done(result) => {
                let moved = !said.is_empty();
                for (name, data) in said {
                    self.publish("egui_pop", request.cause, name, data);
                }
                result.is_ok() && moved
            }
        }
    }

    /// Asks the main window's close (`window.close` of the main window):
    /// whether it closed (not vetoed, no window moved into it instead).
    pub(crate) fn ask_main_close(&self, reason: Reason) -> bool {
        self.request_window_close(WindowRequest::new(
            WindowId::MAIN,
            WindowKind::Main,
            "app",
            reason,
        ))
    }

    /// The main window's pass saw the system ask its close: unless a
    /// `window.close` of the main window already ran (it asked the
    /// system), one is asked now; vetoed, or answered by a window moving
    /// into it, the system's close is cancelled.
    pub(crate) fn main_close_seen(&self, ctx: &Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        let quit_asked = self.take_quitting();
        let closing = !lock(&self.0.bus.registry).holds(WindowId::MAIN);
        if quit_asked || closing {
            return;
        }
        if !self.ask_main_close(Reason::User) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        }
    }

    /// `window.registry`'s part on the main window's close: the system is
    /// asked to close it, unless it asked already.
    pub(super) fn registry_close_main(&self) {
        let ctx = self.egui();
        if !ctx.input_for(ViewportId::ROOT, |i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
        ctx.request_repaint_of(ViewportId::ROOT);
    }

    /// The main window asked to its front while a window's content holds
    /// it: that content goes back to a window of its own, a
    /// `window.reparent` with no parent.
    pub(crate) fn ask_home(&self) {
        let Some(id) = self.take_going_home() else {
            return;
        };
        self.request_window_reparent(WindowRequest::new(
            id.as_str(),
            WindowKind::App,
            "app",
            Reason::User,
        ));
    }

    /// The `frame.close` step's default: the main window's `window.close`;
    /// another window's close request to the system, which its content may
    /// refuse before its pass asks `window.close`. Whether a close was
    /// asked.
    pub(crate) fn frame_close(&self, ctx: &Context) -> bool {
        if ctx.viewport_id() == ViewportId::ROOT {
            return self.ask_main_close(Reason::User);
        }
        ctx.send_viewport_cmd(ViewportCommand::Close);
        true
    }
}
