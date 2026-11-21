//! The history page: values bound to the store, each change a step of
//! the application's history, and the history panel's list beside them
//! (a click jumps to a step, a hover previews it, a right-click undoes
//! that one step); a button makes a change as an agent, so its row names
//! who made it, another pins a snapshot as an agent. Snapshots, the
//! preview, "Undo this step" and the replaced steps are the default
//! owner's (the value store's). Ctrl+Z follows the focus
//! (`history::Routing::Focus`): the values work on the application's
//! history, a sketch beside them on a history of its own, and Ctrl+Z
//! undoes in the one the focus is in, whichever step is newest.

use egui::Ui;
use egui_pop::api::history;
use egui_pop::api::history_view::{self, PANEL_HISTORY};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::lock;
use egui_pop::core::param::{self, Mode};
use egui_pop::model::commands::Origin;
use egui_pop::model::history::{APP, ScopeId};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::model::values::{self, Change, Descriptor, Set, Value};

use super::section;
use crate::shell::SHELL;

pub const HISTORY: PluginId = PluginId::from_static("gallery.history");

/// The values the page edits.
pub const EXPOSURE: &str = "gallery/history/exposure";
pub const RADIUS: &str = "gallery/history/radius";
/// The sketch's value, in its own history.
pub const SKETCH_SIZE: &str = "gallery/history/sketch_size";

/// The sketch's history.
pub const SKETCH: ScopeId = ScopeId::from_static("gallery.sketch");
/// The focus surface of the sketch: Ctrl+Z there undoes in [`SKETCH`].
pub const SKETCH_SURFACE: &str = "gallery.history.sketch";
/// The focus surface of the values: Ctrl+Z there undoes in [`APP`].
pub const VALUES_SURFACE: &str = "gallery.history.values";

fn sketch_size() -> Descriptor {
    Descriptor::int("Sketch size", 1, 64, 8).unit("px")
}

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
            for (path, d) in descriptors()
                .into_iter()
                .chain([(SKETCH_SIZE, sketch_size())])
            {
                s.declare(&owner, path, d)
                    .map_err(|e| PluginError::Failed(e.to_string()))?;
            }
        }
        // Ctrl+Z goes to the history of the surface in focus.
        history::set_routing(&ctx.app(), history::Routing::Focus);
        ctx.history_target(VALUES_SURFACE, APP);
        // Every page works on the application's history: a click into the
        // page makes it Ctrl+Z's target (the sketch, inside, names its
        // own). Before any click, Ctrl+Z does nothing and says why.
        ctx.history_target(egui_pop::api::workbench::PANEL_MAIN.as_str(), APP);
        ctx.story(
            Story::new(StoryId::from_static("history"), "History", show)
                .description("The application's history: each change a step, Ctrl+Z and Ctrl+Shift+Z, the history panel's list (a click jumps to a step, a hover previews it, a right-click undoes that one step), snapshots, the replaced steps behind a toggle, an agent's change named with its origin; a sketch with a history of its own, Ctrl+Z following the focus.")
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

/// An agent's snapshot of the application's history.
pub fn agent_snapshot(app: &AppHandle) {
    let _ = history::take_snapshot(
        app,
        Some(&history::APP),
        "Before the agent",
        None,
        false,
        &Origin::Agent("helper".into()),
    );
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let store = app.store();
    ui.label("Each change below is a step of the application's history: Ctrl+Z undoes it, Ctrl+Shift+Z redoes it, and a click on a row of the list goes back or forward to that step. Hovering a row previews the values there; a right-click on a step undoes that one step, unless a later step changed the same value. A snapshot pins the values; a click on it restores them as a new step. A change made after an undo replaces the steps undone: \"Show replaced steps\" lists them, and a click goes back to one. The same list is the History panel (View > History), and agents read and drive it with the history.* commands.");
    section(ui, "Values (a drag is one step)");
    // A surface takes all the room it is given: each gets only its rows'
    // (a zero height grows to them), so what is drawn below stays out.
    bounded(ui, |ui| {
        egui_pop::core::focus::scope(ui, VALUES_SURFACE, &[], |ui| {
            for (path, d) in descriptors() {
                param::param(ui, &d, Mode::Bound(&store, path));
            }
        });
    });
    section(ui, "A sketch: Ctrl+Z follows the focus");
    ui.label("The sketch keeps its own history. Ctrl+Z undoes in the history of what has the focus, the sketch's or the values', even when the other has a newer step; with the focus elsewhere, in the one focused last. Ctrl+Shift+Z redoes in the same one.");
    bounded(ui, |ui| {
        egui_pop::core::focus::scope(ui, SKETCH_SURFACE, &[], |ui| {
            let _in_sketch = values::enter_history(&SKETCH);
            param::param(ui, &sketch_size(), Mode::Bound(&store, SKETCH_SIZE));
        });
    });
    let goes = history::target(app, false).map(|t| t.scope());
    ui.label(match goes {
        Some(s) => format!("Ctrl+Z goes to: {s}"),
        None => "Ctrl+Z goes nowhere".into(),
    });
    ui.horizontal(|ui| {
        if ui.button("Double the radius as an agent").clicked() {
            agent_change(app);
        }
        if ui.button("Snapshot as an agent").clicked() {
            agent_snapshot(app);
        }
        if ui.button("Show the History panel").clicked() {
            app.set_panel_visible(&PANEL_HISTORY, true);
        }
    });
    section(ui, "The history");
    history_view::show(ui, app);
}

/// `add` in a box as wide as the room and only as tall as what it draws.
fn bounded(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui(egui::vec2(ui.available_width(), 0.0), add);
}
