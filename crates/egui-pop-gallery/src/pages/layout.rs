//! The layout controls page: VS Code's controls of the title bar (the
//! "Customize layout" button and the area toggles), drawn here too, the
//! keys that toggle the areas, the two layout options as settings, and
//! what each area shows now.

use egui::Ui;
use egui_pop::api::layout_controls::{self, SETTING_SIDE_BAR_POSITION, TOGGLES};
use egui_pop::api::stories::Story;
use egui_pop::api::workbench::{self, SETTING_PANEL_ALIGNMENT};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::param::{self, Mode};
use egui_pop::model::ids::{PluginId, StoryId};

use super::section;
use crate::shell::SHELL;

pub const LAYOUT_PAGE: PluginId = PluginId::from_static("gallery.layout");

/// The layout controls page.
#[derive(Debug, Default)]
pub struct LayoutPage;

impl Plugin for LayoutPage {
    fn id(&self) -> PluginId {
        LAYOUT_PAGE
    }

    fn title(&self) -> String {
        "Layout controls".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, super::workbench::WORKBENCH_PAGE]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("layout"), "Layout controls", show)
                .description("VS Code's layout controls: a toggle per area, filled while it shows, and the layout menu with the arrangements and where the side bars and the panel lie.")
                .group("Framework", 101),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    section(ui, "The controls");
    ui.horizontal(|ui| layout_controls::show(ui, app));
    ui.weak("In the title bar when the gallery draws its own frame (the Window frame page): `FramePlugin::layout_controls(true)`. The button opens the layout menu, also View > Customize layout.");
    let keys: Vec<String> = TOGGLES
        .iter()
        .map(|(area, command, _)| {
            let key = app.keys_for(command).first().cloned().unwrap_or_default();
            let state = if layout_controls::area_shown(app, *area) {
                "shown"
            } else {
                "hidden"
            };
            format!("{} ({key}): {state}", workbench::area_name(*area))
        })
        .collect();
    for line in keys {
        ui.monospace(line);
    }

    section(ui, "Options");
    let store = app.store();
    for setting in [SETTING_SIDE_BAR_POSITION, SETTING_PANEL_ALIGNMENT] {
        let path = format!("settings/{setting}");
        let descriptor = egui_pop::core::lock(&store)
            .get(&path)
            .map(|e| e.descriptor.clone());
        if let Some(d) = descriptor {
            param::param(ui, &d, Mode::Bound(&store, &path));
        }
    }
    ui.weak("Each change is one step of Edit > Undo. The panel lies under the editors by default, as VS Code's.");
}
