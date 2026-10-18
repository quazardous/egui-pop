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
use crate::strings::{self, FAULT_BODY, FAULT_TITLE, RETRY};
use crate::theme::Tokens;

/// A caught panic.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fault {
    /// The boundary's name.
    pub name: String,
    pub message: String,
    /// How many times it failed since the last retry.
    pub count: u32,
}

fn fault_id(id: Id) -> Id {
    id.with("egui_pop_fault")
}

fn registry_id() -> Id {
    Id::new("egui_pop_core::fault::all")
}

/// The panic's message.
fn message(payload: &(dyn std::any::Any + Send)) -> String {
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
    let id = ui.id().with(id);
    let key = fault_id(id);
    if let Some(fault) = ui.ctx().data(|d| d.get_temp::<Fault>(key)) {
        card(ui, key, &fault);
        return None;
    }
    let result = catch_unwind(AssertUnwindSafe(|| ui.scope(add).inner));
    match result {
        Ok(r) => Some(r),
        Err(payload) => {
            let fault = Fault {
                name: name.into(),
                message: message(&*payload),
                count: 1,
            };
            log::error!(target: "egui_pop", "`{name}` panicked: {}", fault.message);
            ui.ctx().data_mut(|d| {
                d.insert_temp(key, fault.clone());
                let mut all: Vec<Id> = d.get_temp(registry_id()).unwrap_or_default();
                if !all.contains(&key) {
                    all.push(key);
                }
                d.insert_temp(registry_id(), all);
            });
            card(ui, key, &fault);
            ui.ctx().request_repaint();
            None
        }
    }
}

/// The faults currently shown, for the debug bridge.
pub fn faults(ctx: &egui::Context) -> Vec<Fault> {
    ctx.data(|d| {
        let all: Vec<Id> = d.get_temp(registry_id()).unwrap_or_default();
        all.into_iter()
            .filter_map(|k| d.get_temp::<Fault>(k))
            .collect()
    })
}

fn card(ui: &mut Ui, key: Id, fault: &Fault) {
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
                ui.ctx().data_mut(|d| d.remove::<Fault>(key));
            }
        });
}
