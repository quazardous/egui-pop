//! Fault boundaries: a panel whose drawing code panics shows an error card
//! instead of taking the application down. The panic is caught
//! (`catch_unwind`), logged, and kept until the user retries.
//!
//! Where panics abort (`panic = "abort"`, or WebAssembly without unwinding)
//! nothing can be caught; the boundary then changes nothing.
//!
//! ```
//! use egui_pop_core::fault;
//!
//! fn panel(ui: &mut egui::Ui) {
//!     fault::boundary(ui, "inspector", "Inspector", |ui| {
//!         ui.label("Drawn by code that might panic");
//!     });
//! }
//! ```

use std::panic::{AssertUnwindSafe, catch_unwind};

use egui::{Id, RichText, Shadow, Ui};
use serde::Serialize;

use crate::kit::{Kit, State};
use crate::state::Slot;
use crate::strings::{self, FAULT_BODY, FAULT_TITLE, RETRY};
use crate::theme::Tokens;

/// A caught panic.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fault {
    /// The boundary's name.
    pub name: String,
    pub message: String,
    /// How many times in a row it failed: a retry that fails again adds
    /// one, a pass that draws without a panic starts again from zero.
    pub count: u32,
}

/// The fault a boundary shows, per boundary.
const FAULT: Slot<Fault> = Slot::new("core.fault");
/// Every boundary that failed.
const FAILED: Slot<Vec<Id>> = Slot::new("core.fault.failed");
/// How many times in a row a boundary failed, per boundary: kept across a
/// retry, removed when the boundary draws without a panic.
const STREAK: Slot<u32> = Slot::new("core.fault.streak");

/// The panic's message.
pub(crate) fn message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic without a message".into()
    }
}

/// Draws `add` inside a boundary named `name`. Returns `None` while the
/// boundary shows its error card.
pub fn boundary<R>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    boundary_reporting(ui, id, name, add, |_| {})
}

/// Draws `add` as [`boundary`] does, and hands a panic it catches to
/// `failed`, once per failure: for the caller to say what failed in its
/// own words (a panel's id, its owner) and to publish it.
pub fn boundary_reporting<R>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    add: impl FnOnce(&mut Ui) -> R,
    failed: impl FnOnce(&Fault),
) -> Option<R> {
    let id = ui.scope_id().with(id);
    let key = FAULT.keyed(id);
    if let Some(fault) = key.get(ui.ctx()) {
        card(ui, key, &fault);
        return None;
    }
    let streak = STREAK.keyed(id);
    let result = catch_unwind(AssertUnwindSafe(|| ui.scope(add).inner));
    match result {
        Ok(r) => {
            if streak.exists(ui.ctx()) {
                streak.remove(ui.ctx());
            }
            Some(r)
        }
        Err(payload) => {
            let count = streak.update(ui.ctx(), |n| {
                *n += 1;
                *n
            });
            let fault = Fault {
                name: name.into(),
                message: message(&*payload),
                count,
            };
            log::error!(target: "egui_pop", "`{name}` panicked: {}", fault.message);
            failed(&fault);
            key.set(ui.ctx(), fault.clone());
            FAILED.update(ui.ctx(), |all| {
                if !all.contains(&id) {
                    all.push(id);
                }
            });
            card(ui, key, &fault);
            ui.ctx().request_repaint();
            None
        }
    }
}

/// The faults currently shown, for the debug bridge.
pub fn faults(ctx: &egui::Context) -> Vec<Fault> {
    FAILED
        .get_or_default(ctx)
        .into_iter()
        .filter_map(|id| FAULT.keyed(id).get(ctx))
        .collect()
}

fn card(ui: &mut Ui, key: Slot<Fault>, fault: &Fault) {
    let t = Tokens::of(ui.ctx());
    Kit::of(ui.ctx())
        .fault_card
        .show(ui, State::REST, Shadow::NONE, |ui| {
            ui.label(
                RichText::new(strings::tr_args(
                    ui.ctx(),
                    &FAULT_TITLE,
                    &[("name", &fault.name)],
                ))
                .color(t.status.danger)
                .strong(),
            );
            ui.label(strings::tr(ui.ctx(), &FAULT_BODY));
            ui.label(
                RichText::new(&fault.message)
                    .monospace()
                    .color(t.text.muted),
            );
            if ui.button(strings::tr(ui.ctx(), &RETRY)).clicked() {
                key.remove(ui.ctx());
            }
        });
}
