//! The values page: the darktable-style slider and the parameter widget
//! in its three binding modes (plain, bound to the store, watched), with
//! an int, a choice and a colour bound the same way.

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::lock;
use egui_pop::core::param::{self, Mode};
use egui_pop::core::state::Slot;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::model::values::{Descriptor, Value};

use super::section;
use crate::shell::SHELL;

/// The plain value's own copy, per page.
const PLAIN: Slot<f64> = Slot::new("gallery.values.plain");

pub const VALUES: PluginId = PluginId::from_static("gallery.values");

fn exposure() -> Descriptor {
    Descriptor::float("Exposure", -18.0, 18.0, 0.0)
        .soft(-3.0, 4.0)
        .step(0.01)
        .unit("EV")
}

fn descriptors() -> Vec<(&'static str, Descriptor)> {
    vec![
        ("gallery/values/exposure", exposure()),
        (
            "gallery/values/radius",
            Descriptor::int("Radius", 0, 500, 20)
                .soft(0.0, 100.0)
                .unit("px"),
        ),
        (
            "gallery/values/mode",
            Descriptor::choice("Blend mode", &["normal", "multiply", "screen"], "normal"),
        ),
        (
            "gallery/values/tint",
            Descriptor::floats("Tint", 3, 0.0, 1.0, &[0.85, 0.55, 0.25]),
        ),
        ("gallery/values/preview", Descriptor::bool("Preview", true)),
    ]
}

/// The values page.
#[derive(Debug, Default)]
pub struct ValuesPage;

impl Plugin for ValuesPage {
    fn id(&self) -> PluginId {
        VALUES
    }

    fn title(&self) -> String {
        "Values".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
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
            Story::new(StoryId::from_static("values"), "Values", show)
                .description("The slider (soft and hard range, fine and coarse, reset, typed entry) and the parameter widget, plain, bound or watched.")
                .group("Components", 45),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let store = app.store();
    ui.label("Drag (Ctrl: finer, Shift: coarser; Escape puts the value back), arrows when focused, the wheel when focused or with Alt, Enter to type a value with its unit, double click to reset.");
    section(ui, "Bound to the store (a drag is one undo step)");
    for (path, d) in descriptors() {
        param::param(ui, &d, Mode::Bound(&store, path));
    }
    section(ui, "Watched: the same exposure, shown only");
    let watched = Descriptor::float("Exposure (watched)", -18.0, 18.0, 0.0)
        .soft(-3.0, 4.0)
        .step(0.01)
        .unit("EV");
    param::param(ui, &watched, Mode::Watch(&store, "gallery/values/exposure"));
    section(ui, "Plain: the caller's own value");
    let id = PLAIN.keyed(ui.scope_id());
    let mut plain = id.get(ui.ctx()).map_or(Value::Float(0.5), Value::Float);
    let opacity = Descriptor::float("Opacity", 0.0, 1.0, 1.0)
        .display(100.0, 0)
        .unit("%");
    param::param(ui, &opacity, Mode::Plain(&mut plain));
    if let Some(v) = plain.as_f64() {
        id.set(ui.ctx(), v);
    }
}
