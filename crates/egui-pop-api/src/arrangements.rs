//! Arrangements, as a plugin ([`ArrangementsPlugin`], in
//! `DefaultPlugins`): named layouts of the workbench, saved, switched
//! and reset. They are kept in the host's store
//! (`settings/workbench.arrangements`, one versioned layout document per
//! name, and `settings/workbench.arrangement`, the current name), so they
//! are saved with the settings, or wherever a host keeps its store.
//!
//! Applications offer **templates**, named layouts of their own
//! (`Ctx::arrangement`, owned by the plugin): switching to one starts from it, and resetting the
//! current arrangement goes back to its template (or to every panel in
//! its default area). A saved layout is repaired against today's panels
//! when switched to (ADR 0003: what changed is said).
//!
//! Steps (ADR 0011), each with its default:
//!
//! | Step | Arguments |
//! |---|---|
//! | `arrangement.save` | `{ "name" }` |
//! | `arrangement.switch` | `{ "name" }` |
//! | `arrangement.reset` | `{ "name" }` (the current one) |
//! | `arrangement.delete` | `{ "name" }` |

use std::collections::BTreeMap;

use egui_pop_core::lock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId, StepId};
use egui_pop_model::layout::{self, Layout};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::steps::{Outcome, StepSpec};
use crate::{Ctx, Plugin, PluginError};

/// Where the saved arrangements are: a JSON object, name to layout
/// document.
pub const ARRANGEMENTS: &str = "settings/workbench.arrangements";
/// The current arrangement's name.
pub const CURRENT: &str = "settings/workbench.arrangement";

pub const STEP_SAVE: StepId = StepId::from_static("arrangement.save");
pub const STEP_SWITCH: StepId = StepId::from_static("arrangement.switch");
pub const STEP_RESET: StepId = StepId::from_static("arrangement.reset");
pub const STEP_DELETE: StepId = StepId::from_static("arrangement.delete");

pub const CMD_SAVE: CommandId = CommandId::from_static("workbench.arrangement.save");
pub const CMD_SWITCH: CommandId = CommandId::from_static("workbench.arrangement.switch");
pub const CMD_RESET: CommandId = CommandId::from_static("workbench.arrangement.reset");

/// The name of the arrangement every application starts with.
pub const DEFAULT: &str = "Default";

/// The plugin.
#[derive(Debug, Default)]
pub struct ArrangementsPlugin;

/// The saved arrangements, by name.
pub fn saved(app: &AppHandle) -> BTreeMap<String, Layout> {
    let text = lock(&app.store())
        .get(ARRANGEMENTS)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .unwrap_or_default();
    let map: BTreeMap<String, Json> = serde_json::from_str(&text).unwrap_or_default();
    map.into_iter()
        .filter_map(|(name, doc)| Layout::from_json(&doc.to_string()).ok().map(|l| (name, l)))
        .collect()
}

/// Every name to switch to: the saved ones and the templates.
pub fn names(app: &AppHandle) -> Vec<String> {
    let mut v: Vec<String> = saved(app).into_keys().collect();
    for t in app.workbench_mut().templates.iter().map(|t| t.1.clone()) {
        if !v.contains(&t) {
            v.push(t);
        }
    }
    if !v.iter().any(|n| n == DEFAULT) {
        v.insert(0, DEFAULT.into());
    }
    v
}

/// The current arrangement's name.
pub fn current(app: &AppHandle) -> String {
    lock(&app.store())
        .get(CURRENT)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT.into())
}

fn write(app: &AppHandle, path: &str, text: String) -> Result<(), String> {
    lock(&app.store())
        .set(
            Set::new(path, Change::To(Value::Text(text)), Origin::User)
                .untracked()
                .force(),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn write_saved(app: &AppHandle, map: &BTreeMap<String, Layout>) -> Result<(), String> {
    let docs: BTreeMap<&String, Json> = map
        .iter()
        .map(|(n, l)| {
            let doc = layout::schema().save(l).unwrap_or_default();
            (n, serde_json::from_str(&doc).unwrap_or(Json::Null))
        })
        .collect();
    write(app, ARRANGEMENTS, json!(docs).to_string())
}

fn name_of(args: &Json) -> Result<String, String> {
    let n = args
        .get("name")
        .and_then(Json::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if n.is_empty() {
        Err("an arrangement needs a name".into())
    } else {
        Ok(n.to_string())
    }
}

/// Saves the workbench's layout as `name` (`arrangement.save`).
pub fn save(app: &AppHandle, name: &str) -> Outcome {
    app.step(&STEP_SAVE, json!({ "name": name }), |app, args| {
        let name = name_of(args)?;
        let mut map = saved(app);
        map.insert(name.clone(), app.layout());
        write_saved(app, &map)?;
        write(app, CURRENT, name.clone())?;
        Ok(json!({ "saved": name }))
    })
}

/// Switches to an arrangement (`arrangement.switch`): a saved one, else
/// a template.
pub fn switch(app: &AppHandle, name: &str) -> Outcome {
    app.step(&STEP_SWITCH, json!({ "name": name }), |app, args| {
        let name = name_of(args)?;
        let layout = saved(app)
            .remove(&name)
            .or_else(|| app.workbench_mut().template(&name))
            .or_else(|| (name == DEFAULT).then(Layout::default))
            .ok_or_else(|| format!("no arrangement `{name}`"))?;
        let notes = app.set_layout(layout);
        write(app, CURRENT, name.clone())?;
        Ok(json!({ "switched": name, "repaired": notes }))
    })
}

/// Puts the current arrangement back as its template has it (or every
/// panel in its default area), forgetting what was saved under its name
/// (`arrangement.reset`).
pub fn reset(app: &AppHandle) -> Outcome {
    let name = current(app);
    app.step(&STEP_RESET, json!({ "name": name }), |app, args| {
        let name = name_of(args)?;
        let layout = app.workbench_mut().template(&name).unwrap_or_default();
        let mut map = saved(app);
        map.remove(&name);
        write_saved(app, &map)?;
        let notes = app.set_layout(layout);
        Ok(json!({ "reset": name, "repaired": notes }))
    })
}

/// Forgets a saved arrangement (`arrangement.delete`).
pub fn delete(app: &AppHandle, name: &str) -> Outcome {
    app.step(&STEP_DELETE, json!({ "name": name }), |app, args| {
        let name = name_of(args)?;
        let mut map = saved(app);
        if map.remove(&name).is_none() {
            return Err(format!("no saved arrangement `{name}`"));
        }
        write_saved(app, &map)?;
        if current(app) == name {
            write(app, CURRENT, DEFAULT.into())?;
        }
        Ok(json!({ "deleted": name }))
    })
}

fn outcome(o: Outcome) -> Result<Json, String> {
    match o {
        Outcome::Done(r) => r,
        Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
    }
}

impl Plugin for ArrangementsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.arrangements")
    }

    fn title(&self) -> String {
        "Arrangements".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let owner = ctx.owner().clone();
        {
            let store = ctx.app().store();
            let mut s = lock(&store);
            s.declare(&owner, ARRANGEMENTS, Descriptor::text("Arrangements", "{}"))
                .map_err(|e| PluginError::Failed(e.to_string()))?;
            s.declare(&owner, CURRENT, Descriptor::text("Arrangement", DEFAULT))
                .map_err(|e| PluginError::Failed(e.to_string()))?;
        }
        for spec in [
            StepSpec::new(STEP_SAVE, "The workbench's layout is saved under a name.")
                .args(json!({ "name": "Writing" })),
            StepSpec::new(
                STEP_SWITCH,
                "The workbench switches to a named arrangement.",
            )
            .args(json!({ "name": "Writing" })),
            StepSpec::new(
                STEP_RESET,
                "The current arrangement goes back to its template.",
            )
            .args(json!({ "name": "Writing" })),
            StepSpec::new(STEP_DELETE, "A saved arrangement is forgotten.")
                .args(json!({ "name": "Writing" })),
        ] {
            ctx.step(spec)?;
        }
        ctx.command(
            CommandSpec::new(CMD_SAVE, "Save the arrangement as...", "View")
                .description("Saves the panels' places under a name.")
                .args(json!({ "name": "" }))
                .menu(MENU_VIEW, "3_layout", 10),
            |call| {
                let name = call
                    .args
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                outcome(save(call.app, name))
            },
        )?
        .command(
            CommandSpec::new(CMD_SWITCH, "Switch arrangement", "View")
                .description("Puts the panels as a saved arrangement or a template has them.")
                .args(json!({ "name": "" }))
                .menu(MENU_VIEW, "3_layout", 11),
            |call| {
                let name = call
                    .args
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                outcome(switch(call.app, name))
            },
        )?
        .command(
            CommandSpec::new(CMD_RESET, "Reset the arrangement", "View")
                .description("Puts the panels back as the current arrangement's template has them.")
                .menu(MENU_VIEW, "3_layout", 12),
            |call| outcome(reset(call.app)),
        )?;
        Ok(())
    }
}
