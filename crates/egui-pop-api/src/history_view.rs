//! The history's projection (ADR 0014): the `History` panel and the
//! `history.*` commands agents drive it with, brought by
//! [`HistoryPlugin`].
//!
//! - **The panel** ([`PANEL_HISTORY`], closed until asked for; [`show`]
//!   draws it anywhere): the scopes there are, as chips (the one Ctrl+Z
//!   goes to first), then the chosen scope's steps, oldest first, under
//!   a "Start" row. The head is the selected row; the steps a redo would
//!   make again are muted under it. A click on a row jumps there
//!   (`edit.jump`, vetoable like an undo); a step made by an agent, a
//!   script or another process says so after its label.
//! - **The commands** (agents and scripts; kept out of the palette,
//!   where Edit > Undo and Redo already are): `history.list` (every
//!   scope, whether it is active, where Undo and Redo go and what they
//!   would do), `history.read` (a scope's steps), `history.undo`,
//!   `history.redo` and `history.jump`, each with `scope` (none: where
//!   the keys go), `if_rev` and `dry_run`. They run through the same
//!   steps as the keys, so a hook vetoing an undo vetoes an agent's too.
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_api::history_view::HistoryPlugin;
//! use egui_pop_core::lock;
//! use egui_pop_model::commands::Origin;
//! use egui_pop_model::ids::{CommandId, OwnerId};
//! use egui_pop_model::values::{Change, Descriptor, Set, Value};
//! use serde_json::json;
//!
//! let mut app = App::builder().plugin(HistoryPlugin::default())
//!     .build(&egui::Context::default()).unwrap();
//! let store = app.handle().store();
//! lock(&store).declare(&OwnerId::USER, "doc/n", Descriptor::int("N", 0, 9, 0)).unwrap();
//! lock(&store).set(Set::new("doc/n", Change::To(Value::Int(3)), Origin::User)).unwrap();
//! let read = app
//!     .run(&CommandId::from_static("history.read"), json!({ "scope": "app" }), Origin::Agent("a".into()))
//!     .now().unwrap().unwrap();
//! let head = read["head"].as_u64().unwrap();
//! // A dry run says what an undo would take back, and changes nothing.
//! let dry = app
//!     .run(
//!         &CommandId::from_static("history.undo"),
//!         json!({ "scope": "app", "if_rev": read["rev"], "dry_run": true }),
//!         Origin::Agent("a".into()),
//!     )
//!     .now().unwrap().unwrap();
//! assert_eq!(dry["steps"][0]["id"].as_u64(), Some(head));
//! assert_eq!(lock(&store).get("doc/n").unwrap().value, Value::Int(3));
//! ```

use egui::{RichText, Ui};
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_core::widgets;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::history::{self, Line, ScopeId, StepInfo, Walk};
use crate::host::source_of;
use crate::steps::Outcome;
use crate::strings as txt;
use crate::{Ctx, Plugin, PluginError};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.history");

/// The history panel.
pub const PANEL_HISTORY: PanelId = PanelId::from_static("egui_pop.history");

/// Every scope, whether it is active, where Undo and Redo go.
pub const CMD_LIST: CommandId = CommandId::from_static("history.list");
/// A scope's steps.
pub const CMD_READ: CommandId = CommandId::from_static("history.read");
/// Undoes in a scope.
pub const CMD_UNDO: CommandId = CommandId::from_static("history.undo");
/// Redoes in a scope.
pub const CMD_REDO: CommandId = CommandId::from_static("history.redo");
/// Brings a scope to one of its steps.
pub const CMD_JUMP: CommandId = CommandId::from_static("history.jump");

/// The scope the panel shows, once the user picked one.
const CHOSEN: Slot<ScopeId> = Slot::new("egui_pop.history.scope");

/// The history panel and the `history.*` commands.
#[derive(Clone, Debug)]
pub struct HistoryPlugin {
    panel: bool,
}

impl Default for HistoryPlugin {
    fn default() -> Self {
        Self { panel: true }
    }
}

impl HistoryPlugin {
    /// Without the panel: the commands only (an application drawing
    /// [`show`] where it wants).
    pub fn without_panel(mut self) -> Self {
        self.panel = false;
        self
    }
}

impl Plugin for HistoryPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "History".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let category = "Edit";
        ctx.command(
            CommandSpec::new(CMD_LIST, "List the histories", category)
                .description("Every history (its scope: id, title, whether it is local or durable, its cap), whether it is active in the window, its revision, head and counts, and where Undo and Redo go now with what they would do.")
                .hidden(),
            |call| Ok(history::snapshot(call.app)),
        )?
        .command(
            CommandSpec::new(CMD_READ, "Read a history", category)
                .description("A history's steps (`scope`; none: where Undo goes now): `undo`, oldest first, and `redo`, the next one last; each step's id, label and its text in the interface's language, origin, group, place and time; the history's revision and head.")
                .args(json!({ "scope": "app" }))
                .hidden(),
            |call| {
                let scope = scope_arg(call.app, call.args, Walk::Undo)?;
                let line = history::line(call.app, &scope)
                    .ok_or_else(|| format!("no history `{scope}`"))?;
                Ok(read(call.app, &scope, &line))
            },
        )?
        .command(
            CommandSpec::new(CMD_UNDO, "Undo in a history", category)
                .description("Undoes the last step of a history (`scope`; none: where Undo goes now), refused unless its revision is still `if_rev`; with `dry_run`, only says what it would undo.")
                .args(json!({ "scope": "app", "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| walk(call.app, call.args, Walk::Undo, call.origin),
        )?
        .command(
            CommandSpec::new(CMD_REDO, "Redo in a history", category)
                .description("Redoes the last undone step of a history (`scope`; none: where Redo goes now), refused unless its revision is still `if_rev`; with `dry_run`, only says what it would redo.")
                .args(json!({ "scope": "app", "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| walk(call.app, call.args, Walk::Redo, call.origin),
        )?
        .command(
            CommandSpec::new(CMD_JUMP, "Jump in a history", category)
                .description("Brings a history (`scope`) to one of its steps (`to`, its id; 0: the start) by undos or redos, all or nothing, refused unless its revision is still `if_rev`; with `dry_run`, only says which steps it would walk.")
                .args(json!({ "scope": "app", "to": 0, "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| {
                let to = call.args["to"]
                    .as_u64()
                    .ok_or_else(|| "`to` is missing: a step's id, or 0".to_string())?;
                walk(call.app, call.args, Walk::Jump(to), call.origin)
            },
        )?;
        if self.panel {
            ctx.panel(
                PanelSpec::new(PANEL_HISTORY, "History", Area::EndSide)
                    .min_size(160.0)
                    .default_size(240.0)
                    .icon("history")
                    .closed(),
                |ui, app| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| show(ui, app));
                },
            )?;
        }
        Ok(())
    }
}

/// The `scope` argument, else where the keys go now.
fn scope_arg(app: &AppHandle, args: &Json, what: Walk) -> Result<ScopeId, String> {
    match args.get("scope").and_then(Json::as_str) {
        Some(s) => ScopeId::new(s).map_err(|e| format!("`scope`: {e}")),
        None => history::target(app, what == Walk::Redo)
            .map(|t| t.scope())
            .ok_or_else(|| "no history to read".to_string()),
    }
}

/// An undo, a redo or a jump asked by a command.
fn walk(app: &AppHandle, args: &Json, what: Walk, origin: &Origin) -> Result<Json, String> {
    let scope = match args.get("scope").and_then(Json::as_str) {
        Some(s) => Some(ScopeId::new(s).map_err(|e| format!("`scope`: {e}"))?),
        None => None,
    };
    let if_rev = args.get("if_rev").and_then(Json::as_u64);
    let dry_run = args["dry_run"] == true;
    match history::ask(app, what, scope.as_ref(), if_rev, dry_run, origin) {
        Outcome::Done(r) => r,
        Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
    }
}

/// A step with its label in the interface's language.
fn step_json(ctx: &egui::Context, step: &StepInfo) -> Json {
    let mut v = json!(step);
    v["text"] = json!(history::label_text(ctx, &step.label));
    v
}

/// What `history.read` answers.
fn read(app: &AppHandle, scope: &ScopeId, line: &Line) -> Json {
    let ctx = app.egui();
    json!({
        "scope": scope,
        "rev": line.rev,
        "head": line.head_id(),
        "undo": line.undo.iter().map(|s| step_json(&ctx, s)).collect::<Vec<_>>(),
        "redo": line.redo.iter().map(|s| step_json(&ctx, s)).collect::<Vec<_>>(),
    })
}

/// The scope the panel shows: the one picked, while it is there; else
/// where Ctrl+Z goes; else the first.
pub fn shown_scope(app: &AppHandle) -> Option<ScopeId> {
    let all = history::scopes(app);
    let ctx = app.egui();
    CHOSEN
        .get(&ctx)
        .filter(|s| all.iter().any(|i| i.id == *s))
        .or_else(|| history::target(app, false).map(|t| t.scope()))
        .filter(|s| all.iter().any(|i| i.id == *s))
        .or_else(|| all.first().map(|s| s.id.clone()))
}

/// Shows `scope` in the panel from now on.
pub fn choose(app: &AppHandle, scope: ScopeId) {
    CHOSEN.set(&app.egui(), scope);
}

/// What a step's row says: its label, and who made it when it was not
/// the user.
pub fn row_text(ctx: &egui::Context, step: &StepInfo) -> String {
    let label = history::label_text(ctx, &step.label);
    match step.origin {
        Origin::User => label,
        _ => strings::tr_args(
            ctx,
            &txt::HISTORY_BY,
            &[("label", &label), ("origin", &source_of(&step.origin))],
        ),
    }
}

/// The history panel's content: the scopes as chips when there are
/// several, then the shown scope's steps under "Start"; a click on a row
/// jumps there.
pub fn show(ui: &mut Ui, app: &AppHandle) {
    let ctx = ui.ctx().clone();
    let all = history::scopes(app);
    let Some(scope) = shown_scope(app) else {
        ui.weak(strings::tr(&ctx, &txt::HISTORY_NONE));
        return;
    };
    if all.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            for s in &all {
                if ui
                    .add(widgets::SelectableRow::new(s.id == scope, s.title.as_str()))
                    .clicked()
                {
                    choose(app, s.id.clone());
                }
            }
        });
        ui.separator();
    }
    let line = history::line(app, &scope).unwrap_or_default();
    let muted = Tokens::of(&ctx).text.muted;
    let hint = strings::tr(&ctx, &txt::HISTORY_JUMP);
    let mut to = None;
    let start = strings::tr(&ctx, &txt::HISTORY_START);
    if ui
        .add(widgets::SelectableRow::new(line.undo.is_empty(), start.as_str()).steady())
        .on_hover_text(&hint)
        .clicked()
        && !line.undo.is_empty()
    {
        to = Some(0);
    }
    let head = line.head_id();
    for step in &line.undo {
        let here = step.id == head;
        if ui
            .add(widgets::SelectableRow::new(here, row_text(&ctx, step)).steady())
            .on_hover_text(&hint)
            .clicked()
            && !here
        {
            to = Some(step.id);
        }
    }
    for step in line.redo.iter().rev() {
        let text = RichText::new(row_text(&ctx, step)).color(muted);
        if ui
            .add(widgets::SelectableRow::new(false, text).steady())
            .on_hover_text(&hint)
            .clicked()
        {
            to = Some(step.id);
        }
    }
    if let Some(to) = to {
        let _ = history::jump(app, &scope, to, &Origin::User);
    }
}
