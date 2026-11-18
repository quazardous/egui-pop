//! The detached panels' windows on the bus (ADR 0029, slice 4): the
//! workbench owns them.
//!
//! - Detaching a panel into a window of its own is that window's
//!   `pop.window.open` (`panel:<id>`, kind [`WindowKind::Panel`], owner
//!   [`WORKBENCH_OWNER`]).
//! - Docking a detached panel back (`workbench.attach`, or moving it to an
//!   area, as a tab dragged there does) is its window's `pop.window.reparent`
//!   into the main window.
//! - Closing a detached panel's window (its title bar, the system's
//!   close, `workbench.close_window`, or anyone asking `pop.window.close` of
//!   it) is its `pop.window.close`; its panels dock back.
//!
//! The workbench's listener on the three points ([`WORKBENCH_WINDOW`])
//! changes the layout, whoever asked, before the registry holds or lets
//! go of the window. A workbench change that adds, removes or renames a
//! window some other way (a panel joining another window's tabs, a layout
//! put in, panels declared again) asks the same requests in the same call,
//! once the layout changed: the listener then finds nothing left to do.
//! While a window's content holds the main window, the detached panels are
//! not drawn: the registry lets their windows go with that move and holds
//! them again when the content goes back.
//!
//! The raw viewport calls of the detached panels' windows are here, and
//! nowhere in the workbench.

use bus_contract::{Bus, ListenOpts};
use egui::{Context, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};
use egui_pop_model::ids::PanelId;
use egui_pop_model::layout::Auxiliary;
use serde_json::json;

use super::{
    CHILDREN, FOCUS, REGISTRY, Reason, WindowClose, WindowKind, WindowOpen, WindowReparent,
    WindowRequest, asking,
};
use crate::app::AppHandle;
use crate::window_invariants::PANEL_PREFIX;
use crate::windows::WindowId;
use crate::workbench::Change;

/// What owns the detached panels' windows.
pub const WORKBENCH_OWNER: &str = "egui_pop.workbench";
/// The workbench's listener on the window points: the layout follows the
/// request, whoever asked.
pub const WORKBENCH_WINDOW: &str = "pop.workbench.window";

/// A detached panel's window id in the registry (`panel:<id>`).
pub(crate) fn window_of_panel(panel: &PanelId) -> String {
    format!("{PANEL_PREFIX}{}", panel.as_str())
}

/// The panel naming a detached panel's window, from its registry id.
fn panel_of_window(id: &str) -> Option<PanelId> {
    PanelId::new(id.strip_prefix(PANEL_PREFIX)?).ok()
}

/// A request about a detached panel's window, the workbench asking.
fn request(id: &str, reason: Reason) -> WindowRequest {
    WindowRequest::new(id, WindowKind::Panel, WORKBENCH_OWNER, reason)
}

/// Registers the workbench's listener on `pop.window.open`, `pop.window.close`
/// and `pop.window.reparent`.
pub(super) fn install(kernel: &dyn Bus) {
    let mine = || ListenOpts::new().owner(WORKBENCH_OWNER);
    let listened = [
        kernel.listen::<WindowOpen>(WORKBENCH_WINDOW, mine().before(REGISTRY), |req, did| {
            let Some((app, panel)) = panel_request(req) else {
                return Ok(());
            };
            // Nothing to do when the layout has the window already: the
            // panel alone in it, or a window the workbench made that the
            // registry does not hold yet (a tab group detached whole).
            let named = app
                .workbench_mut()
                .layout
                .window_of(&panel)
                .filter(|a| a.panel == panel)
                .map(|a| a.tabs.is_empty());
            let made = match named {
                Some(true) => true,
                Some(false) => !app.windows_list().iter().any(|w| w.id == req.window),
                None => false,
            };
            if !made {
                app.workbench_mut().apply(&Change::Detach(panel.clone()))?;
                did.push(format!("{WORKBENCH_WINDOW}: `{panel}` detached"));
            }
            Ok(())
        }),
        kernel.listen::<WindowClose>(
            WORKBENCH_WINDOW,
            mine().after(CHILDREN).before(FOCUS),
            |req, did| dock_back(req, did, "its window closed"),
        ),
        kernel.listen::<WindowReparent>(WORKBENCH_WINDOW, mine().before(REGISTRY), |req, did| {
            if req.kind == WindowKind::Panel && req.parent.as_deref() != Some(WindowId::MAIN) {
                return Err(format!(
                    "`{}` moves only into the main window; detaching a panel is `pop.window.open`",
                    req.window
                )
                .into());
            }
            dock_back(req, did, "docked back")
        }),
    ];
    for l in listened {
        debug_assert!(l.is_ok(), "{l:?}");
    }
}

/// A request about a detached panel's window: the application asking,
/// and the panel naming the window.
fn panel_request(req: &WindowRequest) -> Option<(AppHandle, PanelId)> {
    if req.kind != WindowKind::Panel {
        return None;
    }
    Some((asking()?, panel_of_window(&req.window)?))
}

/// `pop.workbench.window` on a window closed or moved into the main window:
/// its panels dock back in their areas, while the layout still has it.
fn dock_back(
    req: &WindowRequest,
    did: &mut Vec<String>,
    how: &str,
) -> Result<(), bus_contract::HookError> {
    let Some((app, panel)) = panel_request(req) else {
        return Ok(());
    };
    let held = app
        .workbench_mut()
        .layout
        .auxiliary
        .iter()
        .any(|a| a.panel == panel);
    if held {
        app.workbench_mut()
            .apply(&Change::CloseWindow(panel.clone()))?;
        did.push(format!("{WORKBENCH_WINDOW}: `{panel}` {how}"));
    }
    Ok(())
}

/// The request a workbench change asks first, when it opens, docks back
/// or closes one detached panel's window: its result as the change's
/// step returns it. `None`: the change is done by the workbench itself
/// (then [`AppHandle::sync_panel_windows`] asks what it changed).
pub(crate) fn ask(app: &AppHandle, change: &Change) -> Option<Result<serde_json::Value, String>> {
    // The detached panels are not drawn while a window's content holds
    // the main window: their windows are not on screen to ask about.
    if app.root_has_content() {
        return None;
    }
    let (panel, window) = {
        let wb = app.workbench_mut();
        let panel = match change {
            Change::Detach(p) | Change::Attach(p) | Change::CloseWindow(p) => p.clone(),
            _ => return None,
        };
        // Refused or unknown: the workbench says why.
        let spec = wb.spec(&panel).cloned()?;
        let window = wb.layout.window_of(&panel).cloned();
        if let Change::Detach(_) = change
            && spec
                .refuses(&crate::mounts::panel_host(
                    egui_pop_model::panels::Place::Window,
                ))
                .is_some()
        {
            return None;
        }
        (panel, window)
    };
    match change {
        Change::Detach(_) => {
            if window
                .as_ref()
                .is_some_and(|a| a.panel == panel && a.tabs.is_empty())
            {
                return None;
            }
            app.request_panel_window_open(&panel, Reason::Command);
            let detached = app.workbench_mut().layout.window_of(&panel).is_some();
            Some(if detached {
                Ok(json!({ "panel": panel, "detached": true }))
            } else {
                Err(format!("`{panel}` was not detached"))
            })
        }
        Change::Attach(_) => {
            // A tab among others: the window stays (the workbench moves it).
            let a = window.filter(|a| a.tabs.is_empty())?;
            let req = request(&window_of_panel(&a.panel), Reason::Command).parent(WindowId::MAIN);
            app.request_window_reparent(req);
            let docked = app.workbench_mut().layout.window_of(&panel).is_none();
            Some(if docked {
                Ok(json!({ "panel": panel, "detached": false }))
            } else {
                Err(format!(
                    "`{panel}` stays in its window: its move was refused"
                ))
            })
        }
        _ => {
            let a = window?;
            let panels: Vec<PanelId> = a.panels().cloned().collect();
            let closed =
                app.request_window_close(request(&window_of_panel(&a.panel), Reason::User));
            Some(if closed {
                Ok(json!({ "panel": panel, "detached": false, "docked": panels }))
            } else {
                Err(format!(
                    "the window of `{panel}` stays: its close was refused"
                ))
            })
        }
    }
}

/// `pop.window.registry`'s part when a detached panel's window leaves it:
/// the backend lists its viewport until the main pass after the one that
/// last showed it.
pub(super) fn left(app: &AppHandle, id: &str) {
    if let Some(v) = crate::window_invariants::viewport_of(WindowKind::Panel, id) {
        app.excuse_backend_once(v, false);
    }
}

/// The ids of the detached panels' windows the main window shows: none
/// while a window's content holds it.
pub(crate) fn shown(app: &AppHandle) -> Vec<String> {
    if app.root_has_content() {
        return Vec::new();
    }
    app.workbench_mut()
        .layout
        .auxiliary
        .iter()
        .map(|a| window_of_panel(&a.panel))
        .collect()
}

/// `pop.window.registry`'s part when a window's content moves into the main
/// window (`into_main`), or back out: the detached panels' windows go
/// from the screen with the shell, and come back with it. What it did.
pub(super) fn follow_shell(
    app: &AppHandle,
    registry: &mut super::Registry,
    into_main: bool,
) -> Option<String> {
    if into_main {
        let before = registry.records.len();
        registry.records.retain(|r| r.kind != WindowKind::Panel);
        let gone = before - registry.records.len();
        (gone > 0).then(|| format!("{REGISTRY}: {gone} detached panels' windows hidden"))
    } else {
        let back = shown(app);
        for id in &back {
            let viewport = crate::window_invariants::viewport_of(WindowKind::Panel, id);
            registry.open(&request(id, Reason::Command), viewport);
        }
        (!back.is_empty()).then(|| {
            format!(
                "{REGISTRY}: {} detached panels' windows shown again",
                back.len()
            )
        })
    }
}

impl AppHandle {
    /// Asks `pop.window.open` of a panel's own window, the workbench asking:
    /// its listener detaches the panel, the registry holds the window, and
    /// `window.opened` follows.
    fn request_panel_window_open(&self, panel: &PanelId, reason: Reason) {
        let id = window_of_panel(panel);
        let did = self.ask_window_open(request(&id, reason));
        self.publish(
            "egui_pop",
            None,
            "window.opened",
            json!({ "window": id, "kind": WindowKind::Panel, "owner": WORKBENCH_OWNER, "reason": reason, "did": did }),
        );
    }

    /// The detached panels' windows after a workbench change made by the
    /// workbench itself (`before`: the windows it had): each window come
    /// is its `pop.window.open`; each gone its `pop.window.reparent` into the main
    /// window when one of its panels is docked now, else its
    /// `pop.window.close`. Asked in the call that changed the layout, never
    /// from a later pass.
    pub(crate) fn sync_panel_windows(&self, before: &[Auxiliary]) {
        if self.root_has_content() {
            return;
        }
        let shown = shown(self);
        let held: Vec<String> = self
            .windows_list()
            .into_iter()
            .filter(|w| w.kind == WindowKind::Panel)
            .map(|w| w.id)
            .collect();
        for id in held.iter().filter(|id| !shown.contains(id)) {
            let docked = before
                .iter()
                .find(|a| window_of_panel(&a.panel) == *id)
                .is_some_and(|a| {
                    let wb = self.workbench_mut();
                    a.panels().any(|p| wb.layout.area_of(p).is_some())
                });
            let req = request(id, Reason::Command);
            if docked {
                self.request_window_reparent(req.parent(WindowId::MAIN));
            } else {
                self.request_window_close(req);
            }
        }
        for id in shown.iter().filter(|id| !held.contains(id)) {
            if let Some(panel) = panel_of_window(id) {
                self.request_panel_window_open(&panel, Reason::Command);
            }
        }
    }
}

/// Shows a detached panel's window for this pass: an immediate viewport
/// (the registry holds it, `pop.window.open` asked it).
pub(crate) fn show_window<T>(
    ctx: &Context,
    viewport: ViewportId,
    builder: ViewportBuilder,
    content: impl FnMut(&mut egui::Ui, ViewportClass) -> T,
) -> T {
    ctx.show_viewport_immediate(viewport, builder, content)
}

/// A detached panel's window given another place by the application on
/// purpose (a layout applied): what changed from `from` to `to` is asked
/// of the system once, each command logged with its reason (#5241). A
/// place the system told is never asked back: the window's builder keeps
/// the place it opened with.
pub(crate) fn ask_place(
    ctx: &Context,
    viewport: ViewportId,
    panel: &PanelId,
    from: crate::window::WindowState,
    to: crate::window::WindowState,
) {
    let mut commands = Vec::new();
    if to.fullscreen != from.fullscreen {
        commands.push(ViewportCommand::Fullscreen(to.fullscreen));
    }
    if to.maximized != from.maximized {
        commands.push(ViewportCommand::Maximized(to.maximized));
    }
    if !to.maximized && !to.fullscreen {
        if to.size != from.size {
            commands.push(ViewportCommand::InnerSize(to.size));
        }
        if let Some(at) = to.position
            && Some(at) != from.position
        {
            commands.push(ViewportCommand::OuterPosition(at));
        }
    }
    for c in commands {
        log::info!(
            target: "egui_pop::window_size",
            "`{panel}`'s window asked {c:?}: the application changed its place in the layout"
        );
        ctx.send_viewport_cmd_to(viewport, c);
    }
}

/// The system asked a detached panel's window to close and its
/// `pop.window.close` was refused: the system's close is cancelled.
pub(crate) fn keep_open(vctx: &Context) {
    vctx.send_viewport_cmd(ViewportCommand::CancelClose);
}
