//! The settings form: every declared setting, by section, applied live,
//! each with its reset.

use egui::Ui;
use egui_pop_core::binding::{self, Binding};
use egui_pop_core::lock;
use egui_pop_core::strings::{self, RESET};
use egui_pop_model::commands::Origin;
use egui_pop_model::values::{Change, Set, Type, Value};

use crate::app::AppHandle;

/// Draws the settings form.
pub fn form(ui: &mut Ui, app: &AppHandle) {
    let store = app.store();
    let mut section = String::new();
    for entry in app.settings() {
        if entry.section != section {
            section = entry.section.clone();
            ui.add_space(6.0);
            ui.strong(&section);
        }
        let d = &entry.descriptor;
        let path = entry.path.as_str();
        ui.horizontal(|ui| {
            ui.label(&d.label).on_hover_text(&d.description);
            match d.ty {
                Type::Bool => {
                    binding::edit(ui, Binding::bound(&store, path), |ui, v: &mut bool| {
                        ui.checkbox(v, "")
                    });
                }
                Type::Float => {
                    let (lo, hi) = d.soft.or(d.hard).unwrap_or((0.0, 1.0));
                    binding::edit(ui, Binding::bound(&store, path), |ui, v: &mut f64| {
                        let mut s = egui::Slider::new(v, lo..=hi).step_by(d.step);
                        if let Some(u) = &d.unit {
                            s = s.suffix(format!(" {u}"));
                        }
                        ui.add(s)
                    });
                }
                Type::Int => {
                    let (lo, hi) = d.hard.unwrap_or((0.0, 100.0));
                    binding::edit(ui, Binding::bound(&store, path), |ui, v: &mut i64| {
                        ui.add(egui::Slider::new(v, lo as i64..=hi as i64))
                    });
                }
                Type::Text => {
                    binding::edit(ui, Binding::bound(&store, path), |ui, v: &mut String| {
                        ui.text_edit_singleline(v)
                    });
                }
                Type::Choice => {
                    let current = lock(&store)
                        .get(path)
                        .and_then(|e| e.value.as_choice().map(str::to_string))
                        .unwrap_or_default();
                    let mut picked = current.clone();
                    egui::ComboBox::from_id_salt(path)
                        .selected_text(&current)
                        .show_ui(ui, |ui| {
                            for c in &d.choices {
                                ui.selectable_value(&mut picked, c.clone(), c);
                            }
                        });
                    if picked != current {
                        let _ = lock(&store).set(Set::new(
                            path,
                            Change::To(Value::Choice(picked)),
                            Origin::User,
                        ));
                    }
                }
                Type::Floats | Type::Ints => {
                    let shown = lock(&store)
                        .get(path)
                        .map(|e| format!("{}", e.value.to_json()))
                        .unwrap_or_default();
                    ui.monospace(shown);
                }
            }
            let changed = lock(&store)
                .get(path)
                .is_some_and(|e| e.value != e.descriptor.default);
            let reset = strings::tr(ui.ctx(), &RESET);
            if ui
                .add_enabled(changed, egui::Button::new(&reset).small())
                .on_hover_text(format!("{reset} {}", d.label))
                .clicked()
            {
                let _ = lock(&store).set(Set::new(path, Change::Reset, Origin::User));
            }
        });
    }
}
