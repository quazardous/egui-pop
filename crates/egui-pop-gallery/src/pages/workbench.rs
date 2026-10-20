//! The workbench page: demonstration panels with placement rules,
//! opened, moved and closed through the workbench's steps, the layout as
//! it stands, and a veto on one move.

use egui::Ui;
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::workbench::{self, STEP_MOVE};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::model::ids::{PanelId, PluginId, StoryId};
use egui_pop::model::panels::{Area, PanelSpec};

use super::section;
use crate::shell::SHELL;

pub const WORKBENCH_PAGE: PluginId = PluginId::from_static("gallery.workbench");

pub const OUTLINE: PanelId = PanelId::from_static("gallery.outline");
pub const PROPERTIES: PanelId = PanelId::from_static("gallery.properties");
pub const CONSOLE: PanelId = PanelId::from_static("gallery.console");
pub const NOTES: PanelId = PanelId::from_static("gallery.notes");

/// The context key the move veto reads.
const VETO: &str = "galleryVetoMoves";

/// The workbench page and its panels.
#[derive(Debug, Default)]
pub struct WorkbenchPage;

impl Plugin for WorkbenchPage {
    fn id(&self) -> PluginId {
        WORKBENCH_PAGE
    }

    fn title(&self) -> String {
        "Workbench".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(
            PanelSpec::new(OUTLINE, "Outline", Area::StartSide)
                .only_in(&[Area::StartSide, Area::EndSide])
                .min_size(140.0)
                .default_size(200.0)
                .icon("file")
                .closed(),
            |ui, _| {
                ui.label("Sections of the document.");
            },
        )?
        .panel(
            PanelSpec::new(PROPERTIES, "Properties", Area::EndSide)
                .min_size(180.0)
                .icon("settings")
                .closed(),
            |ui, _| {
                ui.label("Properties of the selection.");
            },
        )?
        .panel(
            PanelSpec::new(CONSOLE, "Console", Area::Bottom)
                .only_in(&[Area::Bottom])
                .min_size(80.0)
                .default_size(140.0)
                .icon("command")
                .closed(),
            |ui, _| {
                ui.monospace("> ready");
            },
        )?
        .panel(
            PanelSpec::new(NOTES, "Notes", Area::Bottom)
                .icon("edit")
                .closed(),
            |ui, _| {
                ui.label("Free notes.");
            },
        )?;
        ctx.before(STEP_MOVE, |app, _| {
            if app.context().get(VETO) == "true" {
                Before::Veto("moves are vetoed on the workbench page".into())
            } else {
                Before::Go
            }
        });
        ctx.story(
            Story::new(StoryId::from_static("workbench"), "Workbench", show)
                .description("Panels in areas over egui_dock, each with its placement rules (where it may go, its smallest and default sizes), changed by named steps another plugin may veto.")
                .group("Framework", 102),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open the demonstration panels").clicked() {
            for p in [OUTLINE, PROPERTIES, CONSOLE, NOTES] {
                app.set_panel_visible(&p, true);
            }
        }
        if ui.button("Close them").clicked() {
            for p in [OUTLINE, PROPERTIES, CONSOLE, NOTES] {
                app.set_panel_visible(&p, false);
            }
        }
        if ui.button("Move Outline to the end side").clicked() {
            app.move_panel(&OUTLINE, Area::EndSide);
        }
        if ui.button("Move Console to the start side").clicked() {
            app.move_panel(&CONSOLE, Area::StartSide);
        }
        if ui.button("Detach Notes into a window").clicked() {
            app.detach_panel(&NOTES);
        }
        if ui.button("Dock Notes back").clicked() {
            app.attach_panel(&NOTES);
        }
    });
    let mut veto = app.context().get(VETO) == "true";
    if widgets::checkbox(
        ui,
        &mut veto,
        "Veto moves (a before-hook on workbench.move)",
    )
    .changed()
    {
        app.set_context(VETO, if veto { "true" } else { "false" });
    }
    ui.weak("Drag a tab onto another area: it goes there when its rules allow it; the drop hint says why not.");
    // The bottom panel's alignment, a setting (as VS Code's).
    let store = app.store();
    let path = format!("settings/{}", workbench::SETTING_PANEL_ALIGNMENT);
    let descriptor = egui_pop::core::lock(&store)
        .get(&path)
        .map(|e| e.descriptor.clone());
    if let Some(d) = descriptor {
        egui_pop::core::param::param(ui, &d, egui_pop::core::param::Mode::Bound(&store, &path));
    }

    section(ui, "Placement rules");
    egui::Grid::new("gallery.workbench.rules")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Panel");
            ui.strong("Default area");
            ui.strong("May go to");
            ui.strong("Sizes");
            ui.end_row();
            for spec in app.panels() {
                if spec.bare {
                    continue;
                }
                ui.label(&spec.title);
                ui.label(spec.area.name());
                ui.label(if spec.areas.is_empty() {
                    "any area".to_string()
                } else {
                    spec.areas
                        .iter()
                        .map(|a| a.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                ui.label(format!(
                    "min {} pt{}",
                    spec.min_size,
                    spec.default_size
                        .map(|d| format!(", default {d} pt"))
                        .unwrap_or_default()
                ));
                ui.end_row();
            }
        });

    section(ui, "The layout");
    let layout = app.layout();
    for (area, node) in &layout.areas {
        ui.horizontal(|ui| {
            ui.strong(workbench::area_name(*area));
            ui.monospace(
                node.panels()
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        });
    }
    for aux in &layout.auxiliary {
        ui.horizontal(|ui| {
            ui.strong("window");
            ui.monospace(format!("{} on {} at {:?}", aux.panel, aux.screen, aux.rect));
        });
    }
    if !layout.hidden.is_empty() {
        ui.horizontal(|ui| {
            ui.strong("closed");
            ui.monospace(
                layout
                    .hidden
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        });
    }

    section(ui, "The workbench's steps");
    let runs: Vec<String> = app
        .step_traces()
        .into_iter()
        .filter(|t| t.step.as_str().starts_with("workbench."))
        .rev()
        .take(4)
        .map(|t| match (&t.vetoed_by, &t.result) {
            (Some((by, why)), _) => format!("{} {}: vetoed by {by} ({why})", t.step, t.args),
            (None, Some(Ok(_))) => format!("{} {}: done", t.step, t.args),
            (None, Some(Err(e))) => format!("{} {}: refused ({e})", t.step, t.args),
            _ => t.step.to_string(),
        })
        .collect();
    if runs.is_empty() {
        ui.weak("Nothing yet.");
    }
    for r in runs {
        ui.monospace(r);
    }
}
