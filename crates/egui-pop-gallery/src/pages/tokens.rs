//! Every resolved token, by tier.

use egui::Ui;
use egui_pop::theme::{Tokens, color32};
use egui_pop::tokens::{Tier, Value};

use super::chip;

/// The table's filters.
#[derive(Clone, Debug)]
pub struct State {
    /// Only paths containing this text.
    pub filter: String,
    pub reference: bool,
    pub system: bool,
    pub component: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            filter: String::new(),
            reference: true,
            system: true,
            component: true,
        }
    }
}

pub fn playground(ui: &mut Ui, state: &mut State) {
    ui.label("Filter");
    ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text("path contains…"));
    ui.checkbox(&mut state.reference, "Reference (palette)");
    ui.checkbox(&mut state.system, "System (roles)");
    ui.checkbox(&mut state.component, "Component");
}

pub fn show(ui: &mut Ui, state: &mut State) {
    let t = Tokens::of(ui.ctx());
    let r = t.resolved();
    if !r.problems().is_empty() {
        for p in r.problems() {
            ui.colored_label(t.status.danger, p.to_string());
        }
        ui.separator();
    }
    let rows: Vec<_> = r
        .iter()
        .filter(|tok| match tok.tier() {
            Tier::Reference => state.reference,
            Tier::System => state.system,
            Tier::Component => state.component,
        })
        .filter(|tok| tok.path.contains(state.filter.trim()))
        .collect();
    ui.weak(format!("{} of {} tokens", rows.len(), r.len()));
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("tokens.table")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    for head in ["", "Token", "Tier", "Value", "Alias of"] {
                        ui.strong(head);
                    }
                    ui.end_row();
                    for tok in rows {
                        match &tok.value {
                            Value::Color(c) => {
                                chip(ui, color32(*c));
                            }
                            _ => {
                                ui.label("");
                            }
                        }
                        let path = ui.monospace(&tok.path);
                        if let Some(d) = &tok.description {
                            path.on_hover_text(d);
                        }
                        ui.label(tok.tier().name());
                        ui.monospace(tok.value.display());
                        match &tok.alias {
                            Some(a) => ui.monospace(format!("{{{a}}}")),
                            None => ui.weak("-"),
                        };
                        ui.end_row();
                    }
                });
        });
}
