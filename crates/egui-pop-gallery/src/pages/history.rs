//! The history page: values bound to the store, each change a step of
//! the application's history, and the history panel's list beside them
//! (a click jumps to a step); a button makes a change as an agent, so
//! its row names who made it.

use egui::Ui;
use egui_pop::api::history_view::{self, PANEL_HISTORY};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::lock;
use egui_pop::core::param::{self, Mode};
use egui_pop::model::commands::Origin;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::model::values::{Change, Descriptor, Set, Value};

use super::section;
use crate::shell::SHELL;

pub const HISTORY: PluginId = PluginId::from_static("gallery.history");

/// The values the page edits.
pub const EXPOSURE: &str = "gallery/history/exposure";
pub const RADIUS: &str = "gallery/history/radius";

fn descriptors() -> Vec<(&'static str, Descriptor)> {
    vec![
        (
            EXPOSURE,
            Descriptor::float("Exposure", -18.0, 18.0, 0.0)
                .soft(-3.0, 4.0)
                .step(0.01)
                .unit("EV"),
        ),
        (
            RADIUS,
            Descriptor::int("Radius", 0, 500, 20)
                .soft(0.0, 100.0)
                .unit("px"),
        ),
    ]
}

/// The history page.
#[derive(Debug, Default)]
pub struct HistoryPage;

impl Plugin for HistoryPage {
    fn id(&self) -> PluginId {
        HISTORY
    }

    fn title(&self) -> String {
        "History".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, history_view::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let owner = ctx.owner().clone();
        {
            let store = ctx.app().store();
            let mut s = lock(&store);
            for (path, d) in descriptors() {
                s.declare(&owner, path, d)
                    .map_err(|e| PluginError::Failed(e.to_string()))?;
            }
        }
        ctx.story(
            Story::new(StoryId::from_static("history"), "History", show)
                .description("The application's history: each change a step, Ctrl+Z and Ctrl+Shift+Z, the history panel's list (a click jumps to a step), an agent's change named with its origin.")
                .group("Components", 46),
        )?;
        Ok(())
    }
}

/// An agent's change: the radius doubled, named as the agent's.
pub fn agent_change(app: &AppHandle) {
    let store = app.store();
    let mut s = lock(&store);
    let radius = s.get(RADIUS).and_then(|e| e.value.as_f64()).unwrap_or(20.0);
    let _ = s.set(Set::new(
        RADIUS,
        Change::To(Value::Int((radius as i64 * 2).clamp(1, 500))),
        Origin::Agent("helper".into()),
    ));
    s.seal();
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let store = app.store();
    ui.label("Each change below is a step of the application's history: Ctrl+Z undoes it, Ctrl+Shift+Z redoes it, and a click on a row of the list goes back or forward to that step. The same list is the History panel (View > History), and agents read and drive it with the history.* commands.");
    section(ui, "Values (a drag is one step)");
    for (path, d) in descriptors() {
        param::param(ui, &d, Mode::Bound(&store, path));
    }
    ui.horizontal(|ui| {
        if ui.button("Double the radius as an agent").clicked() {
            agent_change(app);
        }
        if ui.button("Show the History panel").clicked() {
            app.set_panel_visible(&PANEL_HISTORY, true);
        }
    });
    section(ui, "The history");
    history_view::show(ui, app);
}
