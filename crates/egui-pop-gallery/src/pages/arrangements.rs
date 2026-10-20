//! The arrangements page: named layouts saved, switched and reset, two
//! templates of the gallery's own, and a veto on switching.

use std::sync::Mutex;

use egui::Ui;
use egui_pop::api::arrangements::{self, STEP_SWITCH};
use egui_pop::api::steps::{Before, Outcome};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::state::Slot;
use egui_pop::core::widgets;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::model::layout::{Layout, Node};
use egui_pop::model::panels::Area;

use super::section;
use super::workbench::{CONSOLE, NOTES, OUTLINE, PROPERTIES};
use crate::shell::SHELL;

/// What the last switch said.
const SAID: Slot<String> = Slot::new("gallery.arrangements.said");

pub const ARRANGEMENTS_PAGE: PluginId = PluginId::from_static("gallery.arrangements");

/// The context key the switch veto reads.
const VETO: &str = "galleryVetoSwitch";

/// The page.
#[derive(Debug, Default)]
pub struct ArrangementsPage {
    name: std::sync::Arc<Mutex<String>>,
}

impl Plugin for ArrangementsPage {
    fn id(&self) -> PluginId {
        ARRANGEMENTS_PAGE
    }

    fn title(&self) -> String {
        "Arrangements".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, super::workbench::WORKBENCH_PAGE]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // Two templates: writing (outline and notes), review (properties
        // and console).
        ctx.arrangement(
            "Writing",
            Layout {
                areas: vec![
                    (Area::StartSide, Node::tab(OUTLINE)),
                    (Area::Bottom, Node::tab(NOTES)),
                ],
                hidden: vec![PROPERTIES, CONSOLE],
                ..Layout::default()
            },
        );
        ctx.arrangement(
            "Review",
            Layout {
                areas: vec![
                    (Area::EndSide, Node::tab(PROPERTIES)),
                    (Area::Bottom, Node::tab(CONSOLE)),
                ],
                hidden: vec![OUTLINE, NOTES],
                ..Layout::default()
            },
        );
        ctx.before(STEP_SWITCH, |app, _| {
            if app.context().get(VETO) == "true" {
                Before::Veto("switching is vetoed on the arrangements page".into())
            } else {
                Before::Go
            }
        });
        let name = self.name.clone();
        ctx.story(
            Story::new(StoryId::from_static("arrangements"), "Arrangements", move |ui, app| {
                show(ui, app, &name);
            })
            .description("Named layouts of the workbench, saved, switched and reset through the host's store; templates the application offers.")
            .group("Framework", 102),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle, name: &Mutex<String>) {
    ui.horizontal(|ui| {
        ui.label("Current:");
        ui.strong(arrangements::current(app));
    });
    let mut veto = app.context().get(VETO) == "true";
    if widgets::checkbox(
        ui,
        &mut veto,
        "Veto switching (a before-hook on arrangement.switch)",
    )
    .changed()
    {
        app.set_context(VETO, if veto { "true" } else { "false" });
    }
    section(ui, "Switch to");
    let mut said = None;
    ui.horizontal_wrapped(|ui| {
        for n in arrangements::names(app) {
            if ui.button(&n).clicked() {
                said = Some(arrangements::switch(app, &n));
            }
        }
        if ui.button("Reset the current one").clicked() {
            said = Some(arrangements::reset(app));
        }
    });
    section(ui, "Save the panels' places as");
    let mut n = egui_pop::core::lock(name);
    ui.horizontal(|ui| {
        let label = ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut *n).desired_width(200.0))
            .labelled_by(label.id);
        if ui.button("Save").clicked() {
            said = Some(arrangements::save(app, &n));
        }
    });
    let saved = arrangements::saved(app);
    if !saved.is_empty() {
        section(ui, "Saved");
        for s in saved.keys() {
            ui.horizontal(|ui| {
                ui.label(s);
                if ui.small_button(format!("Delete {s}")).clicked() {
                    said = Some(arrangements::delete(app, s));
                }
            });
        }
    }
    if let Some(o) = said {
        let text = match o {
            Outcome::Done(Ok(v)) => format!("done: {v}"),
            Outcome::Done(Err(e)) => format!("refused: {e}"),
            Outcome::Vetoed { by, reason } => format!("vetoed by {by}: {reason}"),
        };
        SAID.set(ui.ctx(), text);
    }
    if let Some(t) = SAID.get(ui.ctx()) {
        ui.monospace(t);
    }
}
