//! Popups and native windows on the bus (ADR 0029, slice 5): dialogs and
//! file choices.
//!
//! - A popup opens by its `pop.popup.open` ([`AppHandle::request_popup_open`])
//!   and closes by its `pop.popup.close` ([`AppHandle::request_popup_close`]),
//!   whoever asks: its owner when it is answered, a window closing under
//!   it. Its owner's listener on `pop.popup.close` ends what it showed (a
//!   dialog cancelled, a file choice cancelled), in that call.
//! - A native window egui does not paint (the system's file chooser)
//!   opens by its `pop.window.open` ([`AppHandle::request_native_open`]) and
//!   closes by its `pop.window.close`.
//! - A window closing closes its popups and native windows first, each by
//!   its own request (`pop.window.children`, reason `parent-closed`, or `quit`
//!   when the application quits), so none is left with a parent gone.
//! - A window's content moving into the main window takes its popups and
//!   native windows with it: they become the main window's.

use bus_contract::Point as _;
use serde_json::json;

use super::{CHILDREN, PopupClose, PopupOpen, Reason, WindowKind, WindowOpen, WindowRequest, emit};
use crate::app::AppHandle;
use egui_pop_core::lock;

/// `pop.window.children`'s part on any window: its popups and native windows
/// are closed, each by its own request, before it goes.
pub(super) fn close_children(app: &AppHandle, req: &WindowRequest, did: &mut Vec<String>) {
    let reason = if req.reason == Reason::Quit {
        Reason::Quit
    } else {
        Reason::ParentClosed
    };
    let children: Vec<WindowRequest> = app
        .windows_list()
        .into_iter()
        .filter(|w| w.parent.as_deref() == Some(req.window.as_str()))
        .filter(|w| matches!(w.kind, WindowKind::Popup | WindowKind::Native))
        .map(|w| {
            let mut child =
                WindowRequest::new(w.id, w.kind, w.owner, reason).parent(req.window.clone());
            child.cause = req.cause;
            child
        })
        .collect();
    let (mut popups, mut natives) = (0, 0);
    for child in children {
        if child.kind == WindowKind::Popup {
            if app.request_popup_close(child) {
                popups += 1;
            }
        } else if app.request_window_close(child) {
            natives += 1;
        }
    }
    if popups > 0 {
        did.push(format!("{CHILDREN}: {popups} popups closed"));
    }
    if natives > 0 {
        did.push(format!("{CHILDREN}: {natives} native windows closed"));
    }
}

impl AppHandle {
    /// Whether the registry holds `id`.
    fn holds(&self, id: &str) -> bool {
        lock(&self.0.bus.registry).holds(id)
    }

    /// Says a popup or native window request refused: `<family>.refused`.
    fn refuse(&self, family: &str, point: &str, request: &WindowRequest, reason: &str) {
        log::warn!(
            target: "egui_pop",
            "`{point}` of `{}` refused: {reason}",
            request.window
        );
        self.publish(
            "egui_pop",
            request.cause,
            &format!("{family}.refused"),
            json!({ "request": point, "window": request.window, "owner": request.owner, "reason": reason }),
        );
    }

    /// Why an opening request is refused, if it is: of another kind, no
    /// owner, a parent that is not open.
    fn open_refusal(&self, request: &WindowRequest, kind: WindowKind) -> Option<&'static str> {
        if request.kind != kind {
            Some("not of the kind this request opens")
        } else if request.owner.trim().is_empty() {
            Some("no owner")
        } else if !request.parent.as_deref().is_some_and(|p| self.holds(p)) {
            Some("its parent window is not open")
        } else {
            None
        }
    }

    /// The data of a popup or native window's event.
    fn said(request: &WindowRequest, did: Vec<String>) -> serde_json::Value {
        let mut data = json!({
            "window": request.window,
            "kind": request.kind,
            "owner": request.owner,
            "parent": request.parent,
            "reason": request.reason,
            "did": did,
        });
        if let Some(how) = &request.how {
            data["how"] = json!(how);
        }
        if let Some(c) = request.cause {
            data["cause"] = json!(c);
        }
        data
    }

    /// Asks for a popup (`pop.popup.open`, kind [`WindowKind::Popup`]) in its
    /// parent window, which must be open, with an owner: its listeners all
    /// run now, the registry holding it, and `popup.opened` follows. A
    /// popup of that id already held is held again under this request. A
    /// refusal is said as `popup.refused`. Whether it opened.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_popup_open(&self, request: WindowRequest) -> bool {
        if let Some(why) = self.open_refusal(&request, WindowKind::Popup) {
            self.refuse("popup", PopupOpen::ID, &request, why);
            return false;
        }
        let did = emit::<PopupOpen>(self, request.clone());
        self.publish(
            "egui_pop",
            request.cause,
            "popup.opened",
            Self::said(&request, did),
        );
        true
    }

    /// Asks for a popup's close (`pop.popup.close`): its listeners all run
    /// now, its owner's first (what it showed ends: a dialog or a file
    /// choice cancelled), then the registry; `popup.closed` follows. A
    /// popup not open is refused (`popup.refused`). Whether it closed.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_popup_close(&self, request: WindowRequest) -> bool {
        if !self.holds(&request.window) {
            self.refuse(
                "popup",
                PopupClose::ID,
                &request,
                "no open popup of this id",
            );
            return false;
        }
        let did = emit::<PopupClose>(self, request.clone());
        let closed = !self.holds(&request.window);
        self.publish(
            "egui_pop",
            request.cause,
            "popup.closed",
            Self::said(&request, did),
        );
        closed
    }

    /// Asks for a native window egui does not paint (`pop.window.open`, kind
    /// [`WindowKind::Native`]: the system's file chooser, a web view's own
    /// window) as a child of an open window, with an owner: the registry
    /// holds it and `window.opened` follows. It closes by its
    /// `pop.window.close` ([`AppHandle::request_window_close`]), and with its
    /// parent. A refusal is said as `window.refused`. Whether it opened.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_native_open(&self, request: WindowRequest) -> bool {
        if let Some(why) = self.open_refusal(&request, WindowKind::Native) {
            self.refuse("window", WindowOpen::ID, &request, why);
            return false;
        }
        let did = self.ask_window_open(request.clone());
        self.publish(
            "egui_pop",
            request.cause,
            "window.opened",
            Self::said(&request, did),
        );
        true
    }

    /// The registry id of the window a viewport shows: the main window,
    /// an application window, a detached panel's window; the main
    /// window's when none is held for it. The parent to name for a popup
    /// or a native window opened from a pass of that viewport.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn registry_window_of(&self, viewport: egui::ViewportId) -> String {
        let shown = format!("{viewport:?}");
        lock(&self.0.bus.registry)
            .records
            .iter()
            .find(|r| {
                matches!(
                    r.kind,
                    WindowKind::Main | WindowKind::App | WindowKind::Panel
                ) && r.viewport.as_deref() == Some(shown.as_str())
            })
            .map_or_else(
                || crate::windows::WindowId::MAIN.to_string(),
                |r| r.id.clone(),
            )
    }
}
