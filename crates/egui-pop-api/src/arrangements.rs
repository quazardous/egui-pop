//! Arrangements, as a plugin ([`ArrangementsPlugin`], in
//! `DefaultPlugins`): named layouts of the workbench, saved, switched
//! and reset. They are kept in the host's store
//! (`settings/workbench.arrangements`, one versioned layout document per
//! name, and `settings/workbench.arrangement`, the current name), so they
//! are saved with the settings, or wherever a host keeps its store.
//!
//! With the context stack (ADR 0012), the current arrangement is the
//! **arranged frame's**: the last active mode that declares an
//! arrangement (`FrameSpec::arrangement`), else the top place, else the
//! root. The root's name stays in `settings/workbench.arrangement`, as
//! it always was; each place's and mode's in
//! `settings/workbench.arrangement.frames` (a JSON object, frame id to
//! name), written when the user switches or saves there. A frame the
//! user never arranged starts from the arrangement it declares. Settings
//! files written before keep their meaning: their name is the root's.
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
use crate::contexts::{self, FrameId, FrameRegistry, State};
use crate::steps::{Outcome, StepSpec};
use crate::{Ctx, Plugin, PluginError};

/// Where the saved arrangements are: a JSON object, name to layout
/// document.
pub const ARRANGEMENTS: &str = "settings/workbench.arrangements";
/// The current arrangement's name at the root of the context stack (the
/// whole application without it).
pub const CURRENT: &str = "settings/workbench.arrangement";
/// The current arrangement of each place and arranged mode of the
/// context stack: a JSON object, frame id to name.
pub const CURRENT_BY_FRAME: &str = "settings/workbench.arrangement.frames";

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

fn text_of(app: &AppHandle, path: &str) -> String {
    lock(&app.store())
        .get(path)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .unwrap_or_default()
}

/// The current arrangement of each frame the user arranged.
pub fn by_frame(app: &AppHandle) -> BTreeMap<String, String> {
    serde_json::from_str(&text_of(app, CURRENT_BY_FRAME)).unwrap_or_default()
}

/// The frame of the context stack whose arrangement is shown: the last
/// active mode declaring an arrangement, else the top place; `None` at
/// the root (or without the stack). With the arrangement it declares.
pub fn arranged_frame(app: &AppHandle) -> Option<(FrameId, Option<String>)> {
    let svc = app.service(&contexts::CONTEXTS)?;
    arranged_in(&svc.shown(), &svc.frames())
}

/// [`arranged_frame`] of a state.
pub(crate) fn arranged_in(
    state: &State,
    frames: &FrameRegistry,
) -> Option<(FrameId, Option<String>)> {
    let mode = state.modes.iter().rev().find_map(|m| {
        frames
            .get(&m.frame)
            .and_then(|s| s.arrangement.clone())
            .map(|a| (m.frame.clone(), Some(a)))
    });
    mode.or_else(|| {
        state.places.last().map(|e| {
            (
                e.frame.clone(),
                frames.get(&e.frame).and_then(|s| s.arrangement.clone()),
            )
        })
    })
}

/// A frame's current arrangement: the one the user chose there, else the
/// one it declares.
pub(crate) fn of_frame(
    app: &AppHandle,
    frame: &FrameId,
    declared: Option<String>,
) -> Option<String> {
    by_frame(app).remove(frame.as_str()).or(declared)
}

/// The current arrangement's name: the arranged frame's (see
/// [`arranged_frame`]), else the root's.
pub fn current(app: &AppHandle) -> String {
    let name = match arranged_frame(app) {
        Some((frame, declared)) => of_frame(app, &frame, declared).unwrap_or_default(),
        None => text_of(app, CURRENT),
    };
    if name.is_empty() {
        DEFAULT.into()
    } else {
        name
    }
}

/// Makes `name` the current arrangement where the user is.
fn set_current(app: &AppHandle, name: &str) -> Result<(), String> {
    match arranged_frame(app) {
        Some((frame, _)) => {
            let mut map = by_frame(app);
            map.insert(frame.to_string(), name.to_string());
            write(app, CURRENT_BY_FRAME, json!(map).to_string())
        }
        None => write(app, CURRENT, name.to_string()),
    }
}

/// The layout an arrangement names: a saved one, else a template, else
/// every panel in its default area for [`DEFAULT`].
pub fn layout_of(app: &AppHandle, name: &str) -> Option<Layout> {
    saved(app)
        .remove(name)
        .or_else(|| app.workbench_mut().template(name))
        .or_else(|| (name == DEFAULT).then(Layout::default))
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
        set_current(app, &name)?;
        Ok(json!({ "saved": name }))
    })
}

/// Switches to an arrangement (`arrangement.switch`): a saved one, else
/// a template.
pub fn switch(app: &AppHandle, name: &str) -> Outcome {
    app.step(&STEP_SWITCH, json!({ "name": name }), |app, args| {
        let name = name_of(args)?;
        let layout = layout_of(app, &name).ok_or_else(|| format!("no arrangement `{name}`"))?;
        let notes = app.set_layout(layout);
        set_current(app, &name)?;
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
        if text_of(app, CURRENT) == name {
            write(app, CURRENT, DEFAULT.into())?;
        }
        // The frames that were on it go back to what they declare.
        let mut frames = by_frame(app);
        let before = frames.len();
        frames.retain(|_, n| *n != name);
        if frames.len() != before {
            write(app, CURRENT_BY_FRAME, json!(frames).to_string())?;
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
            s.declare(
                &owner,
                CURRENT_BY_FRAME,
                Descriptor::text("Arrangement by place and mode", "{}"),
            )
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
