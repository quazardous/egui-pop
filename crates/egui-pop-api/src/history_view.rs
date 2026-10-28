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
//!   script or another process says so after its label. When the owner
//!   supports them: the snapshots above the steps (a click restores
//!   one), a hover previewing a row, "Undo this step" in a step's
//!   context menu, and the replaced steps behind a toggle.
//! - **The commands** (agents and scripts; kept out of the palette,
//!   where Edit > Undo and Redo already are): `history.list` (every
//!   scope, whether it is active, where Undo and Redo go and what they
//!   would do), `history.read` (a scope's steps), `history.undo`,
//!   `history.redo`, `history.jump`, `history.snapshot`,
//!   `history.restore`, `history.drop_snapshot` and `history.undo_step`,
//!   each with `scope` (none: where the keys go), `if_rev` and
//!   `dry_run`. They run through the same
//!   steps as the keys, so a hook vetoing an undo vetoes an agent's too,
//!   and in the interface, not through the host's dispatch (which would
//!   hold the host, and with it the history owner).
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
use crate::history::{self, At, Line, ScopeId, StepInfo, Walk};
use crate::host::source_of;
use crate::steps::Outcome;
use crate::strings as txt;
use crate::toasts;
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
/// Takes a snapshot of a scope.
pub const CMD_SNAPSHOT: CommandId = CommandId::from_static("history.snapshot");
/// Brings a scope back to one of its snapshots.
pub const CMD_RESTORE: CommandId = CommandId::from_static("history.restore");
/// Deletes a snapshot.
pub const CMD_DROP_SNAPSHOT: CommandId = CommandId::from_static("history.drop_snapshot");
/// Undoes one step of a scope out of order.
pub const CMD_UNDO_STEP: CommandId = CommandId::from_static("history.undo_step");

/// The scope the panel shows, once the user picked one.
const CHOSEN: Slot<ScopeId> = Slot::new("egui_pop.history.scope");

/// Whether the panel shows the steps new steps replaced.
const SHOW_REPLACED: Slot<bool> = Slot::new("egui_pop.history.replaced");

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
                .description("A history's steps (`scope`; none: where Undo goes now): `undo`, oldest first, and `redo`, the next one last; each step's id, label and its text in the interface's language, origin, group, place and time; the history's revision and head; its `snapshots`, the `replaced` steps its owner keeps (each with the step it came `after`) and the `features` its owner supports.")
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
                .icon("undo")
                .description("Undoes the last step of a history (`scope`; none: where Undo goes now), refused unless its revision is still `if_rev`; with `dry_run`, only says what it would undo.")
                .args(json!({ "scope": "app", "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| walk(call.app, call.args, Walk::Undo, call.origin),
        )?
        .command(
            CommandSpec::new(CMD_REDO, "Redo in a history", category)
                .icon("redo")
                .description("Redoes the last undone step of a history (`scope`; none: where Redo goes now), refused unless its revision is still `if_rev`; with `dry_run`, only says what it would redo.")
                .args(json!({ "scope": "app", "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| walk(call.app, call.args, Walk::Redo, call.origin),
        )?
        .command(
            CommandSpec::new(CMD_JUMP, "Jump in a history", category)
                .icon("history")
                .description("Brings a history (`scope`) to one of its steps (`to`, its id; 0: the start) by undos or redos, all or nothing, refused unless its revision is still `if_rev`; with `dry_run`, only says which steps it would walk.")
                .args(json!({ "scope": "app", "to": 0, "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| {
                let to = call.args["to"]
                    .as_u64()
                    .ok_or_else(|| "`to` is missing: a step's id, or 0".to_string())?;
                walk(call.app, call.args, Walk::Jump(to), call.origin)
            },
        )?
        .command(
            CommandSpec::new(CMD_SNAPSHOT, "Take a snapshot", category)
                .description("Pins a history's current state (`scope`; none: where Undo goes now) under `name` (none: numbered), refused unless its revision is still `if_rev`, or when its owner has no snapshots; with `dry_run`, only says what it would take.")
                .args(json!({ "scope": "app", "name": "Before the agent", "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| {
                let (scope, if_rev, dry_run) = request_args(call.args)?;
                let name = match call.args.get("name").and_then(Json::as_str) {
                    Some(n) => n.to_string(),
                    None => next_name(call.app, scope.as_ref()),
                };
                let snap = history::take_snapshot(
                    call.app,
                    scope.as_ref(),
                    &name,
                    if_rev,
                    dry_run,
                    call.origin,
                )
                .map_err(|r| r.reason)?;
                let mut out = json!({ "snapshot": snap });
                if dry_run {
                    out["dry_run"] = json!(true);
                }
                Ok(out)
            },
        )?
        .command(
            CommandSpec::new(CMD_RESTORE, "Restore a snapshot", category)
                .description("Brings a history (`scope`; none: where Undo goes now) back to one of its snapshots (`snapshot`, its id) as one new step an undo takes back, refused unless its revision is still `if_rev`; with `dry_run`, only says what it would change.")
                .args(json!({ "scope": "app", "snapshot": 1, "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| {
                let id = id_arg(call.args, "snapshot", "a snapshot's id")?;
                walk(call.app, call.args, Walk::Restore(id), call.origin)
            },
        )?
        .command(
            CommandSpec::new(CMD_DROP_SNAPSHOT, "Delete a snapshot", category)
                .description("Deletes one of a history's snapshots (`scope`; none: where Undo goes now; `snapshot`, its id), refused unless its revision is still `if_rev`; with `dry_run`, only checks it is there.")
                .args(json!({ "scope": "app", "snapshot": 1, "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| {
                let id = id_arg(call.args, "snapshot", "a snapshot's id")?;
                let (scope, if_rev, dry_run) = request_args(call.args)?;
                history::forget_snapshot(call.app, scope.as_ref(), id, if_rev, dry_run, call.origin)
                    .map_err(|r| r.reason)?;
                Ok(json!({ "deleted": id, "dry_run": dry_run }))
            },
        )?
        .command(
            CommandSpec::new(CMD_UNDO_STEP, "Undo one step", category)
                .description("Undoes one step of a history (`scope`; none: where Undo goes now; `step`, its id) out of order: the newest as an undo does, an older one by a new step putting its values back, refused when a later step changed one of them or when its owner does not allow it, and unless its revision is still `if_rev`; with `dry_run`, only checks it.")
                .args(json!({ "scope": "app", "step": 3, "if_rev": null, "dry_run": false }))
                .hidden(),
            |call| {
                let id = id_arg(call.args, "step", "a step's id")?;
                walk(call.app, call.args, Walk::UndoStep(id), call.origin)
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

/// A request's `scope` (none: where the keys go), `if_rev` and
/// `dry_run`.
fn request_args(args: &Json) -> Result<(Option<ScopeId>, Option<u64>, bool), String> {
    let scope = match args.get("scope").and_then(Json::as_str) {
        Some(s) => Some(ScopeId::new(s).map_err(|e| format!("`scope`: {e}"))?),
        None => None,
    };
    Ok((
        scope,
        args.get("if_rev").and_then(Json::as_u64),
        args["dry_run"] == true,
    ))
}

/// A required id argument.
fn id_arg(args: &Json, name: &str, what: &str) -> Result<u64, String> {
    args[name]
        .as_u64()
        .ok_or_else(|| format!("`{name}` is missing: {what}"))
}

/// A new snapshot's name: "Snapshot 3" after two.
fn next_name(app: &AppHandle, scope: Option<&ScopeId>) -> String {
    let scope = scope
        .cloned()
        .or_else(|| history::target(app, false).map(|t| t.scope()));
    let n = scope.map_or(0, |s| history::snapshots(app, &s).len()) + 1;
    strings::tr_args(
        &app.egui(),
        &txt::HISTORY_SNAPSHOT_NAME,
        &[("n", &n.to_string())],
    )
}

/// An undo, a redo, a jump, a restore or one step undone, asked by a
/// command.
fn walk(app: &AppHandle, args: &Json, what: Walk, origin: &Origin) -> Result<Json, String> {
    let (scope, if_rev, dry_run) = request_args(args)?;
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
    let replaced: Vec<Json> = history::replaced(app, scope)
        .iter()
        .map(|r| {
            let mut v = step_json(&ctx, &r.step);
            v["after"] = json!(r.after);
            v
        })
        .collect();
    json!({
        "scope": scope,
        "rev": line.rev,
        "head": line.head_id(),
        "undo": line.undo.iter().map(|s| step_json(&ctx, s)).collect::<Vec<_>>(),
        "redo": line.redo.iter().map(|s| step_json(&ctx, s)).collect::<Vec<_>>(),
        "snapshots": history::snapshots(app, scope),
        "replaced": replaced,
        "features": history::features(app, scope),
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

/// What a click or a context menu asks of the panel.
enum Act {
    Jump(u64),
    Restore(u64),
    Forget(u64),
    UndoStep(u64),
    Snapshot,
}

/// A row's hover: the point it previews, the owner's answer beside the
/// row's hint.
fn previewed(
    response: egui::Response,
    app: &AppHandle,
    view: egui::Id,
    scope: &ScopeId,
    at: At,
    hint: &str,
) -> egui::Response {
    if !response.hovered() {
        return response.on_hover_text(hint);
    }
    let preview = history::hover_in(app, view, scope, at);
    let changes = preview.map(|p| p.changes).unwrap_or_default();
    if changes.is_empty() {
        return response.on_hover_text(hint);
    }
    response.on_hover_ui(|ui| {
        ui.label(hint);
        ui.separator();
        let ctx = ui.ctx().clone();
        for c in &changes {
            ui.label(strings::tr_args(
                &ctx,
                &txt::HISTORY_PREVIEW_CHANGE,
                &[("what", &c.what), ("then", &c.then), ("now", &c.now)],
            ));
        }
    })
}

/// The history panel's content: the scopes as chips when there are
/// several; the snapshots, when the owner keeps them; then the shown
/// scope's steps under "Start" (the replaced ones behind a toggle). A
/// click on a row jumps there (a snapshot's restores it); hovering a row
/// asks the owner for a preview; a step's context menu undoes that one
/// step, when the owner allows it.
pub fn show(ui: &mut Ui, app: &AppHandle) {
    let view = ui.id();
    rows(ui, app, view);
    // After the rows: a preview none of them asked for in this pass ends.
    history::rows_drawn(app, view);
}

fn rows(ui: &mut Ui, app: &AppHandle, view: egui::Id) {
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
    let features = history::features(app, &scope);
    let snapshots = history::snapshots(app, &scope);
    let replaced = history::replaced(app, &scope);
    let muted = Tokens::of(&ctx).text.muted;
    let hint = strings::tr(&ctx, &txt::HISTORY_JUMP);
    let mut act = None;
    if features.snapshots || !snapshots.is_empty() {
        ui.label(RichText::new(strings::tr(&ctx, &txt::HISTORY_SNAPSHOTS)).strong());
        let restore_hint = strings::tr(&ctx, &txt::HISTORY_RESTORE_HINT);
        for snap in &snapshots {
            let r = ui.add(widgets::SelectableRow::new(false, snap.name.as_str()).steady());
            let r = previewed(r, app, view, &scope, At::Snapshot(snap.id), &restore_hint);
            if r.clicked() {
                act = Some(Act::Restore(snap.id));
            }
            egui_pop_core::pointer::claim::context_menu(&r, |ui| {
                if ui
                    .button(strings::tr(&ctx, &txt::HISTORY_RESTORE))
                    .clicked()
                {
                    act = Some(Act::Restore(snap.id));
                    ui.close();
                }
                if ui
                    .button(strings::tr(&ctx, &txt::HISTORY_DELETE_SNAPSHOT))
                    .clicked()
                {
                    act = Some(Act::Forget(snap.id));
                    ui.close();
                }
            });
        }
        if features.snapshots
            && ui
                .button(strings::tr(&ctx, &txt::HISTORY_TAKE_SNAPSHOT))
                .clicked()
        {
            act = Some(Act::Snapshot);
        }
        ui.separator();
    }
    let show_replaced = !replaced.is_empty() && SHOW_REPLACED.get(&ctx).unwrap_or(false);
    let mut drawn_replaced = vec![false; replaced.len()];
    // The replaced steps whose line left after `after`, under its row.
    let mut replaced_after = |ui: &mut Ui, after: Option<u64>, act: &mut Option<Act>| {
        if !show_replaced {
            return;
        }
        for (i, r) in replaced.iter().enumerate() {
            if drawn_replaced[i] || after.is_some_and(|a| a != r.after) {
                continue;
            }
            drawn_replaced[i] = true;
            let label = history::label_text(&ctx, &r.step.label);
            let text = strings::tr_args(&ctx, &txt::HISTORY_REPLACED, &[("label", &label)]);
            let text = RichText::new(text).color(muted).italics();
            let row = ui.add(widgets::SelectableRow::new(false, text).steady());
            if previewed(row, app, view, &scope, At::Step(r.step.id), &hint).clicked() {
                *act = Some(Act::Jump(r.step.id));
            }
        }
    };
    let start = strings::tr(&ctx, &txt::HISTORY_START);
    let r = ui.add(widgets::SelectableRow::new(line.undo.is_empty(), start.as_str()).steady());
    if previewed(r, app, view, &scope, At::Step(0), &hint).clicked() && !line.undo.is_empty() {
        act = Some(Act::Jump(0));
    }
    replaced_after(ui, Some(0), &mut act);
    let head = line.head_id();
    let undo_this = strings::tr(&ctx, &txt::HISTORY_UNDO_THIS);
    for step in &line.undo {
        let here = step.id == head;
        let r = ui.add(widgets::SelectableRow::new(here, row_text(&ctx, step)).steady());
        let r = previewed(r, app, view, &scope, At::Step(step.id), &hint);
        if r.clicked() && !here {
            act = Some(Act::Jump(step.id));
        }
        if features.undo_step {
            egui_pop_core::pointer::claim::context_menu(&r, |ui| {
                if ui.button(undo_this.as_str()).clicked() {
                    act = Some(Act::UndoStep(step.id));
                    ui.close();
                }
            });
        }
        replaced_after(ui, Some(step.id), &mut act);
    }
    for step in line.redo.iter().rev() {
        let text = RichText::new(row_text(&ctx, step)).color(muted);
        let r = ui.add(widgets::SelectableRow::new(false, text).steady());
        if previewed(r, app, view, &scope, At::Step(step.id), &hint).clicked() {
            act = Some(Act::Jump(step.id));
        }
        replaced_after(ui, Some(step.id), &mut act);
    }
    // Lines that left from a step no longer shown.
    replaced_after(ui, None, &mut act);
    if !replaced.is_empty() {
        let mut on = SHOW_REPLACED.get(&ctx).unwrap_or(false);
        if widgets::checkbox(ui, &mut on, strings::tr(&ctx, &txt::HISTORY_SHOW_REPLACED)).changed()
        {
            SHOW_REPLACED.set(&ctx, on);
        }
    }
    if let Some(act) = act {
        run_act(app, &scope, act);
    }
}

/// Runs what the panel asked, as the user; a refusal says why in a
/// toast.
fn run_act(app: &AppHandle, scope: &ScopeId, act: Act) {
    history::end_preview(app);
    let user = Origin::User;
    let refusal = match act {
        Act::Jump(to) => outcome_error(history::jump(app, scope, to, &user)),
        Act::Restore(id) => outcome_error(history::restore(app, scope, id, &user)),
        Act::UndoStep(id) => outcome_error(history::undo_step(app, scope, id, &user)),
        Act::Forget(id) => history::forget_snapshot(app, Some(scope), id, None, false, &user)
            .err()
            .map(|r| r.reason),
        Act::Snapshot => {
            let name = next_name(app, Some(scope));
            history::take_snapshot(app, Some(scope), &name, None, false, &user)
                .err()
                .map(|r| r.reason)
        }
    };
    if let Some(reason) = refusal
        && let Some(toasts) = app.service(&toasts::TOASTS)
    {
        toasts.notify(toasts::Toast::warning(&reason));
    }
}

fn outcome_error(outcome: Outcome) -> Option<String> {
    match outcome {
        Outcome::Done(Ok(_)) => None,
        Outcome::Done(Err(e)) => Some(e),
        Outcome::Vetoed { reason, .. } => Some(reason),
    }
}
