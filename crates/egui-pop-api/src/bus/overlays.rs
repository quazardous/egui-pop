//! The overlay stack and egui's popups on the bus (ADR 0029, slice 6):
//! menus, the palette, flyouts, popovers, hover cards.
//!
//! - Every overlay of every window's stack (`egui_pop_core::overlay`) and
//!   every egui popup the stack follows (a context menu, a menu bar's
//!   menu, the jobs popover, a hover card) is a popup of the registry: its
//!   opening is its `pop.popup.open`, its closing its `pop.popup.close`, asked in
//!   the call that opened or closed it ([`overlay::watch`]), with how it
//!   closed (`how`: `escape`, `outside`, `chosen`, `owner`, `below`,
//!   `gone`).
//! - A popup's close asked by anyone else (its window closing under it,
//!   `pop.window.children` with `parent-closed`) closes it in the stack
//!   ([`STACK`], a listener of `pop.popup.close`), with what was above it.
//! - An owner names its overlay's popup and itself
//!   ([`AppHandle::name_overlay`]); an overlay not named is
//!   `overlay:<id>@<window>`, owned by [`OVERLAY_OWNER`].
//! - Dialogs and file choices ask their own requests: the stack leaves
//!   theirs alone.
//!
//! Off the bus, as the ADR's exceptions say: egui's tooltips and the
//! lists of a field (a combo box's choices, completions), drawn only while
//! their widget is, per-frame painting.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bus_contract::ListenOpts;
use egui::{Id, ViewportId};
use egui_pop_core::overlay::{self, Change, Closing};

use super::{OWNER, PopupClose, REGISTRY, Reason, WindowKind, WindowRequest, asking};
use crate::app::{AppHandle, WeakApp};

/// The owner of an overlay its owner did not name.
pub const OVERLAY_OWNER: &str = "egui_pop.overlay";
/// Closes in the stack a popup closed by a request (`pop.popup.close`).
pub const STACK: &str = "pop.overlay.stack";
/// The start of an unnamed overlay's popup id.
pub const POPUP_PREFIX: &str = "overlay:";

/// The overlays the bus knows.
#[derive(Debug, Default)]
pub(crate) struct Overlays {
    /// Named by their owner, in a window: the popup id and the owner.
    named: HashMap<(Id, ViewportId), (String, String)>,
    /// Asking their own requests (dialogs, file choices).
    own: HashSet<Id>,
    /// Open: popup id → the overlay.
    open: HashMap<String, (Id, ViewportId)>,
}

thread_local! {
    /// The applications whose context the stack is watched in.
    static WATCHED: RefCell<Vec<WeakApp>> = const { RefCell::new(Vec::new()) };
}

/// Watches the overlays of `ctx` for `app`: each opening and closing is
/// its popup's request.
pub(crate) fn watch(ctx: &egui::Context, app: &AppHandle) {
    WATCHED.with_borrow_mut(|w| {
        w.retain(|a| a.get().is_ok());
        w.push(app.downgrade());
    });
    overlay::watch(
        ctx,
        Some(Arc::new(|ctx: &egui::Context, change: &Change| {
            let app = WATCHED.with_borrow(|w| {
                w.iter()
                    .filter_map(|a| a.get().ok())
                    .find(|a| a.egui() == *ctx)
            });
            if let Some(app) = app {
                app.overlay_changed(change);
            }
        })),
    );
}

/// Registers the stack's listener on `pop.popup.close`.
pub(super) fn install(kernel: &dyn bus_contract::Bus) {
    let l = kernel.listen::<PopupClose>(
        STACK,
        ListenOpts::new().owner(OWNER).before(REGISTRY),
        |req, did| {
            let Some(app) = asking() else { return Ok(()) };
            // Not open, or the stack itself telling of its close.
            let Some((id, window)) = app.0.bus.overlays.borrow_mut().open.remove(&req.window)
            else {
                return Ok(());
            };
            if overlay::close_in_window(&app.egui(), window, id) {
                did.push(format!("{STACK}: closed"));
            }
            Ok(())
        },
    );
    debug_assert!(l.is_ok(), "{l:?}");
    let l = kernel.listen::<PopupClose>(
        crate::palette::POPUP_LISTENER,
        ListenOpts::new().owner(crate::palette::OWNER).before(STACK),
        |req, did| {
            let Some(app) = asking() else { return Ok(()) };
            if crate::palette::popup_closed(&app, &req.window) {
                did.push(format!("{}: closed", crate::palette::POPUP_LISTENER));
            }
            Ok(())
        },
    );
    debug_assert!(l.is_ok(), "{l:?}");
    crate::menus::listen_popup(kernel);
}

/// The request's reason for how an overlay closed.
fn reason_of(how: Closing) -> Reason {
    match how {
        Closing::Escape | Closing::Outside | Closing::Chosen => Reason::User,
        Closing::Below => Reason::ParentClosed,
        _ => Reason::Command,
    }
}

impl AppHandle {
    /// Names the popup an overlay of the stack (or an egui popup it
    /// follows) is in the registry while shown in `window`, and its owner:
    /// `palette@main`, `egui_pop.palette`. Call it before showing it; a
    /// popup not named is `overlay:<id>@<window>`, owned by
    /// [`OVERLAY_OWNER`].
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn name_overlay(&self, id: Id, window: ViewportId, popup: String, owner: &str) {
        let mut o = self.0.bus.overlays.borrow_mut();
        let named = (popup, owner.to_string());
        if o.named.get(&(id, window)) != Some(&named) {
            o.named.insert((id, window), named);
        }
    }

    /// An overlay whose owner asks its popup's requests itself (a dialog,
    /// a file choice): the stack's watch leaves it alone.
    pub(crate) fn overlay_on_its_own(&self, id: Id) {
        self.0.bus.overlays.borrow_mut().own.insert(id);
    }

    /// Closes an overlay by its popup's `pop.popup.close`, the person having
    /// closed it (`how`: Escape, an item chosen), or by itself when the
    /// registry does not hold it. Whether the request closed it.
    pub(crate) fn close_overlay(&self, ctx: &egui::Context, id: Id, how: Closing) -> bool {
        let popup = self.open_popup_of(&self.0.bus.overlays.borrow(), id, ctx.viewport_id());
        let Some(popup) = popup else {
            overlay::close(ctx, id);
            return false;
        };
        let record = self.windows_list().into_iter().find(|w| w.id == popup);
        let Some(record) = record else {
            self.0.bus.overlays.borrow_mut().open.remove(&popup);
            overlay::close(ctx, id);
            return false;
        };
        let mut req = WindowRequest::new(popup, WindowKind::Popup, record.owner, reason_of(how))
            .how(how.as_str());
        req.parent = record.parent;
        self.request_popup_close(req)
    }

    /// The open popup of an overlay shown in `window` (one overlay may be
    /// open in several windows: the palette), else any of that overlay.
    fn open_popup_of(&self, o: &Overlays, id: Id, window: ViewportId) -> Option<String> {
        let named = o.named.get(&(id, window)).map(|n| n.0.clone());
        named
            .filter(|p| o.open.contains_key(p))
            .or_else(|| {
                let here = format!("{POPUP_PREFIX}{id:?}@{}", self.registry_window_of(window));
                o.open.contains_key(&here).then_some(here)
            })
            .or_else(|| {
                o.open
                    .iter()
                    .find(|(_, (i, _))| *i == id)
                    .map(|(p, _)| p.clone())
            })
    }

    /// The popup id and owner of an overlay shown in `window`, unless it
    /// asks its own requests.
    fn overlay_popup(&self, id: Id, window: ViewportId) -> Option<(String, String)> {
        let o = self.0.bus.overlays.borrow();
        if o.own.contains(&id) {
            return None;
        }
        Some(o.named.get(&(id, window)).cloned().unwrap_or_else(|| {
            (
                format!("{POPUP_PREFIX}{id:?}@{}", self.registry_window_of(window)),
                OVERLAY_OWNER.to_string(),
            )
        }))
    }

    /// An overlay opened or closed in the stack: its popup's request.
    fn overlay_changed(&self, change: &Change) {
        let Some(how) = change.closed else {
            let Some((popup, owner)) = self.overlay_popup(change.id, change.window) else {
                return;
            };
            self.0
                .bus
                .overlays
                .borrow_mut()
                .open
                .insert(popup.clone(), (change.id, change.window));
            let req = WindowRequest::new(popup.clone(), WindowKind::Popup, owner, Reason::User)
                .parent(self.registry_window_of(change.window));
            if !self.request_popup_open(req) {
                self.0.bus.overlays.borrow_mut().open.remove(&popup);
            }
            return;
        };
        // Taken off first: the stack's listener leaves it (it is closed).
        let popup = {
            let mut o = self.0.bus.overlays.borrow_mut();
            if o.own.contains(&change.id) {
                return;
            }
            let popup = self.open_popup_of(&o, change.id, change.window);
            if let Some(p) = &popup {
                o.open.remove(p);
            }
            popup
        };
        let Some(popup) = popup else { return };
        let Some(record) = self.windows_list().into_iter().find(|w| w.id == popup) else {
            return;
        };
        let mut req = WindowRequest::new(popup, WindowKind::Popup, record.owner, reason_of(how))
            .how(how.as_str());
        req.parent = record.parent;
        self.request_popup_close(req);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_person_closing_is_user_a_parent_closing_is_parent_closed() {
        for how in [Closing::Escape, Closing::Outside, Closing::Chosen] {
            assert_eq!(reason_of(how), Reason::User);
        }
        assert_eq!(reason_of(Closing::Below), Reason::ParentClosed);
        assert_eq!(reason_of(Closing::Owner), Reason::Command);
        assert_eq!(reason_of(Closing::Gone), Reason::Command);
    }
}
