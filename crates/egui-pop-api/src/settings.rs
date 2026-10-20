//! The settings form: every declared setting, by section, applied live,
//! each with its reset. A grid of label, editor and reset; every editor
//! is labelled by its setting's label for screen readers. Sections,
//! labels, descriptions and a choice's values are shown in the
//! interface's language (`egui_pop_core::strings`).

use egui::{Response, Ui};
use egui_pop_core::binding::{self, Binding};
use egui_pop_core::lock;
use egui_pop_core::strings::{self, RESET};
use egui_pop_core::widgets;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::OwnerId;
use egui_pop_model::values::{Change, Descriptor, Set, Type, Value};

use crate::app::AppHandle;
use crate::hooks::SettingEntry;
use crate::strings as txt;

/// Draws the settings form.
pub fn form(ui: &mut Ui, app: &AppHandle) {
    let entries = app.settings();
    let mut sections: Vec<(String, Vec<SettingEntry>)> = Vec::new();
    let ctx = ui.ctx().clone();
    for e in entries {
        match sections.iter_mut().find(|(s, _)| *s == e.section) {
            Some((_, list)) => list.push(e),
            None => sections.push((e.section.clone(), vec![e])),
        }
    }
    for (section, list) in sections {
        ui.add_space(ui.spacing().item_spacing.y * 2.0);
        if let Some(first) = list.first() {
            ui.strong(app.section_title(&ctx, &first.owner, &section));
        }
        egui::Grid::new(("egui_pop.settings", &section))
            .num_columns(3)
            .spacing([
                ui.spacing().item_spacing.x * 2.0,
                ui.spacing().item_spacing.y,
            ])
            .show(ui, |ui| {
                for entry in list {
                    row(ui, app, &entry);
                    ui.end_row();
                }
            });
    }
}

fn row(ui: &mut Ui, app: &AppHandle, entry: &SettingEntry) {
    let store = app.store();
    let d: &Descriptor = &entry.descriptor;
    let path = entry.path.as_str();
    let ctx = ui.ctx().clone();
    let id = entry.id.as_str();
    let text = strings::setting_label(&ctx, id, &d.label);
    let label = ui.label(&text);
    let label = if d.description.is_empty() {
        label
    } else {
        label.on_hover_text(strings::setting_description(&ctx, id, &d.description))
    };
    let named = |r: Response| r.labelled_by(label.id);
    ui.horizontal(|ui| match d.ty {
        Type::Bool => {
            binding::edit(ui, Binding::bound(&store, path), |ui, v: &mut bool| {
                named(widgets::checkbox(ui, v, ""))
            });
        }
        Type::Float => {
            let (lo, hi) = d.soft.or(d.hard).unwrap_or((0.0, 1.0));
            binding::edit(ui, Binding::bound(&store, path), |ui, v: &mut f64| {
                let slider = named(
                    ui.add(
                        egui::Slider::new(v, lo..=hi)
                            .step_by(d.step)
                            .show_value(false),
                    ),
                );
                let mut value = egui::DragValue::new(v)
                    .range(lo..=hi)
                    .speed(d.step)
                    .max_decimals(d.precision as usize);
                if let Some(u) = &d.unit {
                    value = value.suffix(format!(" {u}"));
                }
                slider | named(ui.add(value))
            });
        }
        Type::Int => {
            let (lo, hi) = d.hard.unwrap_or((0.0, 100.0));
            let (lo, hi) = (lo as i64, hi as i64);
            binding::edit(ui, Binding::bound(&store, path), |ui, v: &mut i64| {
                let slider = named(ui.add(egui::Slider::new(v, lo..=hi).show_value(false)));
                slider | named(ui.add(egui::DragValue::new(v).range(lo..=hi)))
            });
        }
        Type::Text => {
            binding::edit(ui, Binding::bound(&store, path), |ui, v: &mut String| {
                let r = named(ui.text_edit_singleline(v));
                egui_pop_core::focus_mode::follow_pointer(&r);
                r
            });
        }
        Type::Choice => {
            let current = lock(&store)
                .get(path)
                .and_then(|e| e.value.as_choice().map(str::to_string))
                .unwrap_or_default();
            let mut picked = current.clone();
            let r = widgets::combo_box(path)
                .selected_text(strings::setting_choice(&ctx, id, &current))
                .show_ui(ui, |ui| {
                    for c in &d.choices {
                        widgets::selectable_value(
                            ui,
                            &mut picked,
                            c.clone(),
                            strings::setting_choice(&ctx, id, c),
                        );
                    }
                })
                .response;
            named(r);
            if picked != current {
                let _ = lock(&store).set(
                    Set::new(path, Change::To(Value::Choice(picked)), Origin::User)
                        .by(&OwnerId::FRAMEWORK),
                );
            }
        }
        Type::Floats | Type::Ints => {
            let shown = lock(&store)
                .get(path)
                .map(|e| format!("{}", e.value.to_json()))
                .unwrap_or_default();
            ui.monospace(shown);
        }
    });
    let changed = lock(&store)
        .get(path)
        .is_some_and(|e| e.value != e.descriptor.default);
    let reset = strings::tr(ui.ctx(), &RESET);
    let setting = [("setting", text.as_str())];
    if ui
        .add_enabled(changed, egui::Button::new(&reset).small())
        .on_hover_text(strings::tr_args(
            ui.ctx(),
            &txt::SETTINGS_RESET_SETTING,
            &setting,
        ))
        .on_disabled_hover_text(strings::tr_args(
            ui.ctx(),
            &txt::SETTINGS_AT_DEFAULT,
            &setting,
        ))
        .clicked()
    {
        let _ =
            lock(&store).set(Set::new(path, Change::Reset, Origin::User).by(&OwnerId::FRAMEWORK));
    }
}
