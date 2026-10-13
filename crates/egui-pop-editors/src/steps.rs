//! The editors' steps (ADR 0011), their commands (keys, palette, the
//! agent surface), their debug collectors, and the plugin declaring them.
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `edit.change` | `editor`, `kind` (`typing`, `deletion`, `paste`, `cut`, `indentation`, `replace`, `edit`, `undo`, `redo`), `origin`, `version`, `changes` (`[{from, to, text}]`, positions `{line, col}` from 0, applied in order), `selection` (after, or `null`) | applies the changes; the result is the new `version` |
//! | `edit.save` | `editor`, `version`, `lines`, `language`, `origin` | nothing to write: the application writes from a run-hook or an after-hook; once the step succeeds the text is clean |
//! | `code.diagnostics` | `editor`, `version`, `diagnostics` (`[{line, columns, severity, message, source}]`) | replaces the editor's diagnostics |
//! | `code.insert_completion` | `editor`, `item` (`{label, insert, detail, kind, replace}`), `from`, `to`, `text` | replaces `from..to` with `text` through `edit.change` |
//! | `code.accept_suggestion` | `editor`, `at`, `text` | inserts the ghost text at `at` through `edit.change` |
//!
//! Events on the journal (code editors shown with an application):
//! `code.cursor_moved` (`editor`, `line`, `col`), `code.selection_changed`
//! (`editor`, `anchor`, `head`), `code.completion_requested` (`editor`,
//! `id`, `trigger`, `at`), `code.completion_accepted` (`editor`, `label`,
//! `text`); every change is the `edit.change` step's own signal, with its
//! ranges.
//!
//! A before-hook on `edit.change` refuses an edit (a read-only region)
//! or rewrites it (`Redirect` with other `changes`); an after-hook keeps
//! the application's copy of the text in step. An editor shown without
//! an application runs the defaults directly.
//!
//! Each step has a collector on the debug bridge: `edit_changes` (the
//! open editors and the last changes), `edit_saves` (the last saves) and
//! `code_diagnostics` (each editor's diagnostics).

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use egui_pop_api::host::source_of;
use egui_pop_api::services::ServiceToken;
use egui_pop_api::steps::{Outcome, StepSpec};
use egui_pop_api::{AppHandle, Call, Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId, StepId};
use egui_pop_model::keymap::BindOptions;
use serde_json::{Value as Json, json};

use crate::buffer::{Change, Pos};
use crate::history::EditKind;
use crate::markers::Diagnostic;
use crate::model::{EditorState, Why};
use crate::search::Query;
use crate::selection::Selection;

pub const EDIT_CHANGE: StepId = StepId::from_static("edit.change");
pub const EDIT_SAVE: StepId = StepId::from_static("edit.save");
pub const CODE_DIAGNOSTICS: StepId = StepId::from_static("code.diagnostics");
pub const CODE_INSERT_COMPLETION: StepId = StepId::from_static("code.insert_completion");
pub const CODE_ACCEPT_SUGGESTION: StepId = StepId::from_static("code.accept_suggestion");

/// Events published on the journal (not steps: nothing to veto).
pub const EVENT_CURSOR_MOVED: &str = "code.cursor_moved";
pub const EVENT_SELECTION_CHANGED: &str = "code.selection_changed";
pub const EVENT_COMPLETION_REQUESTED: &str = "code.completion_requested";
pub const EVENT_COMPLETION_ACCEPTED: &str = "code.completion_accepted";

/// The plugin's id.
pub const EDITORS_PLUGIN: PluginId = PluginId::from_static("egui_pop.editors");

/// The focus scope every editor draws in: its keys are bound there.
pub const SURFACE: &str = "editor";

pub const CMD_FIND: CommandId = CommandId::from_static("editor.find");
pub const CMD_FIND_NEXT: CommandId = CommandId::from_static("editor.find_next");
pub const CMD_FIND_PREVIOUS: CommandId = CommandId::from_static("editor.find_previous");
pub const CMD_REPLACE: CommandId = CommandId::from_static("editor.replace");
pub const CMD_GOTO_LINE: CommandId = CommandId::from_static("editor.goto_line");
pub const CMD_SAVE: CommandId = CommandId::from_static("editor.save");
pub const CMD_READ: CommandId = CommandId::from_static("editor.read");
pub const CMD_WRITE: CommandId = CommandId::from_static("editor.write");
pub const CMD_SELECT: CommandId = CommandId::from_static("editor.select");
pub const CMD_UNDO: CommandId = CommandId::from_static("editor.undo");
pub const CMD_REDO: CommandId = CommandId::from_static("editor.redo");
pub const CMD_FOLD: CommandId = CommandId::from_static("code.fold");
pub const CMD_UNFOLD: CommandId = CommandId::from_static("code.unfold");
pub const CMD_UNFOLD_ALL: CommandId = CommandId::from_static("code.unfold_all");
pub const CMD_BREAKPOINT: CommandId = CommandId::from_static("code.toggle_breakpoint");
pub const CMD_COMPLETE: CommandId = CommandId::from_static("code.complete");
pub const CMD_ACTIONS: CommandId = CommandId::from_static("code.actions");
pub const CMD_DEFINITION: CommandId = CommandId::from_static("code.definition");
pub const CMD_HOVER: CommandId = CommandId::from_static("code.hover");
pub const CMD_DIAGNOSTICS: CommandId = CommandId::from_static("code.set_diagnostics");

/// How many changes and saves the collectors keep.
pub const LOG: usize = 64;

/// The editors shown with an application, by id: what commands and
/// collectors reach. An editor registers itself when drawn with
/// `.app(...)`; [`Editors::detach`] forgets one.
#[derive(Debug, Default)]
pub struct Editors {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    editors: BTreeMap<String, EditorState>,
    focused: Option<String>,
    changes: VecDeque<Json>,
    saves: VecDeque<Json>,
}

/// The registry, a service of [`EditorsPlugin`].
pub const EDITORS: ServiceToken<Editors> = ServiceToken::new("egui_pop.editors");

impl Editors {
    /// Registers an editor under its id (once; a new state under the
    /// same id replaces the old one).
    pub fn attach(&self, state: &EditorState) {
        let id = state.id();
        let mut i = lock(&self.inner);
        match i.editors.get(&id) {
            Some(s) if s.same(state) => {}
            _ => {
                i.editors.insert(id, state.clone());
            }
        }
    }

    pub fn detach(&self, id: &str) {
        let mut i = lock(&self.inner);
        i.editors.remove(id);
        if i.focused.as_deref() == Some(id) {
            i.focused = None;
        }
    }

    pub fn get(&self, id: &str) -> Option<EditorState> {
        lock(&self.inner).editors.get(id).cloned()
    }

    pub fn ids(&self) -> Vec<String> {
        lock(&self.inner).editors.keys().cloned().collect()
    }

    /// The editor that last had the keyboard focus.
    pub fn focused(&self) -> Option<EditorState> {
        let i = lock(&self.inner);
        i.focused.as_ref().and_then(|f| i.editors.get(f)).cloned()
    }

    pub(crate) fn set_focused(&self, id: &str) {
        let mut i = lock(&self.inner);
        if i.focused.as_deref() != Some(id) {
            i.focused = Some(id.to_string());
        }
    }

    fn log(&self, saves: bool, entry: Json) {
        let mut i = lock(&self.inner);
        let log = if saves { &mut i.saves } else { &mut i.changes };
        log.push_back(entry);
        while log.len() > LOG {
            log.pop_front();
        }
    }

    /// The last changes, oldest first.
    pub fn changes(&self) -> Vec<Json> {
        lock(&self.inner).changes.iter().cloned().collect()
    }

    /// The last saves, oldest first.
    pub fn saves(&self) -> Vec<Json> {
        lock(&self.inner).saves.iter().cloned().collect()
    }

    fn all(&self) -> Vec<EditorState> {
        lock(&self.inner).editors.values().cloned().collect()
    }
}

/// Declares the editors' steps, commands and collectors, and provides
/// the [`Editors`] registry. Connect it when editors run with an
/// application.
#[derive(Debug, Default)]
pub struct EditorsPlugin;

/// Every step the editors declare.
pub fn specs() -> Vec<StepSpec> {
    vec![
        StepSpec::new(
            EDIT_CHANGE,
            "An editor's text changes (typing, a paste, an undo, a command); a veto keeps the text as it is, a redirect changes what is applied.",
        )
        .args(json!({
            "editor": "notes",
            "kind": "typing",
            "origin": "user",
            "version": 3,
            "changes": [{ "from": { "line": 0, "col": 5 }, "to": { "line": 0, "col": 5 }, "text": "!" }],
            "selection": null,
        })),
        StepSpec::new(
            EDIT_SAVE,
            "An editor's text is saved (Ctrl+S); the application writes it, and the text is clean once the step succeeds.",
        )
        .args(json!({ "editor": "main.rs", "version": 7, "lines": 120, "language": "rs", "origin": "user" })),
        StepSpec::new(
            CODE_DIAGNOSTICS,
            "A code editor's diagnostics are replaced (a compiler's, a linter's).",
        )
        .args(json!({
            "editor": "main.rs",
            "version": 7,
            "diagnostics": [{ "line": 3, "columns": [4, 9], "severity": "error", "message": "cannot find value `x`", "source": "rustc" }],
        })),
        StepSpec::new(
            CODE_INSERT_COMPLETION,
            "A completion is inserted (Enter, Tab or a click in the list); a veto keeps the text, a redirect changes what is inserted.",
        )
        .args(json!({
            "editor": "main.rs",
            "item": { "label": "println", "insert": "println!", "detail": "macro", "kind": "function", "replace": null },
            "from": { "line": 4, "col": 4 },
            "to": { "line": 4, "col": 7 },
            "text": "println!",
        })),
        StepSpec::new(
            CODE_ACCEPT_SUGGESTION,
            "An inline suggestion (ghost text) is accepted with Tab.",
        )
        .args(json!({ "editor": "main.rs", "at": { "line": 4, "col": 8 }, "text": "(\"done\");" })),
    ]
}

/// The editor a command acts on: `editor` in its arguments, else the
/// one that last had the focus.
fn target(call: &Call<'_>) -> Result<EditorState, String> {
    let reg = call
        .app
        .service(&EDITORS)
        .ok_or("the editors plugin is not connected")?;
    match call.args.get("editor").and_then(Json::as_str) {
        Some(id) => reg.get(id).ok_or_else(|| format!("no editor `{id}`")),
        None => reg
            .focused()
            .ok_or_else(|| "no editor has had the focus; name one with `editor`".into()),
    }
}

fn pos(v: &Json) -> Option<Pos> {
    serde_json::from_value(v.clone()).ok()
}

fn line_arg(call: &Call<'_>, state: &EditorState) -> usize {
    call.args
        .get("line")
        .and_then(Json::as_u64)
        .map_or_else(|| state.selection().head.line, |l| l as usize)
}

fn query_of(args: &Json, base: &Query) -> Query {
    let flag = |k: &str, d: bool| args.get(k).and_then(Json::as_bool).unwrap_or(d);
    Query {
        text: args
            .get("query")
            .and_then(Json::as_str)
            .map_or_else(|| base.text.clone(), str::to_string),
        case_sensitive: flag("case_sensitive", base.case_sensitive),
        whole_word: flag("whole_word", base.whole_word),
        regex: flag("regex", base.regex),
    }
}

/// Moves to the next or previous match: its index.
pub(crate) fn find_step(state: &EditorState, forward: bool) -> Option<usize> {
    let mut m = state.lock();
    m.refresh_matches();
    let from = if forward {
        m.selection.span().end
    } else {
        m.selection.span().start
    };
    let i = if forward {
        crate::search::next_from(&m.find.matches, from)
    } else {
        crate::search::previous_from(&m.find.matches, from)
    }?;
    let s = m.find.matches[i];
    m.find.current = Some(i);
    m.select(Selection::new(s.start, s.end));
    Some(i)
}

/// Replaces the current match and moves to the next: whether it did.
pub(crate) fn replace_one(
    state: &EditorState,
    app: Option<&AppHandle>,
    origin: &str,
) -> Result<bool, String> {
    let change = {
        let mut m = state.lock();
        m.refresh_matches();
        let sel = m.selection.span();
        let Some(&hit) = m.find.matches.iter().find(|s| **s == sel) else {
            drop(m);
            find_step(state, true);
            return Ok(false);
        };
        let with =
            crate::search::replacement_for(&m.buffer, &m.find.query, hit, &m.find.replacement);
        Change::new(hit.start, hit.end, with)
    };
    state.edit(
        app,
        vec![change],
        Why::Edit(EditKind::Replace),
        None,
        origin,
    )?;
    find_step(state, true);
    Ok(true)
}

/// Opens the search bar of an editor (with replace), focused.
pub(crate) fn open_find(state: &EditorState, replace: bool) {
    let mut m = state.lock();
    m.find.open = true;
    m.find.replace = replace || (m.find.replace && m.find.open);
    m.find.goto = None;
    m.find.focus = true;
    let sel = m.buffer.slice(m.selection.span());
    if !sel.is_empty() && !sel.contains('\n') {
        m.find.query.text = sel;
    }
}

/// Opens the go-to-line field.
pub(crate) fn open_goto(state: &EditorState) {
    let mut m = state.lock();
    m.find.open = true;
    m.find.goto = Some(String::new());
    m.find.focus = true;
}

/// Puts the cursor on line `line` (from 1, as people count).
pub(crate) fn goto(state: &EditorState, line: usize) -> usize {
    let mut m = state.lock();
    let l = line.saturating_sub(1).min(m.buffer.line_count() - 1);
    m.select(Selection::caret(Pos::new(l, 0)));
    m.view.focus = true;
    l
}

/// Folds the block the line is in: the line if it folds, else the
/// nearest line above that folds over it.
pub(crate) fn fold_at(state: &EditorState, line: usize) -> Option<usize> {
    let start = {
        let m = state.lock();
        (0..=line.min(m.buffer.line_count() - 1)).rev().find(|&l| {
            crate::fold::fold_end(&m.buffer, l, m.tab).is_some_and(|e| e >= line)
                && !m.folds.is_folded(l)
        })
    }?;
    state.fold(start).then_some(start)
}

/// Unfolds the fold at or around the line.
pub(crate) fn unfold_at(state: &EditorState, line: usize) -> Option<usize> {
    let mut m = state.lock();
    let start = m.folds.starts().filter(|&s| s <= line).last()?;
    m.folds.unfold(start).then_some(start)
}

fn ok(state: &EditorState) -> Json {
    json!({ "editor": state.id(), "selection": state.selection(), "version": state.version() })
}

impl Plugin for EditorsPlugin {
    fn id(&self) -> PluginId {
        EDITORS_PLUGIN
    }

    fn title(&self) -> String {
        "Editors".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for spec in specs() {
            ctx.step(spec)?;
        }
        ctx.service(&EDITORS, Editors::default())?;
        let reg = ctx.app().service(&EDITORS).expect("just provided");
        commands(ctx)?;
        // The plugin follows its own steps for the collectors.
        let r = reg.clone();
        ctx.after(EDIT_CHANGE, move |_, args, result| {
            let changes = args["changes"].as_array().map_or(0, Vec::len);
            let first = &args["changes"][0];
            let text = first["text"].as_str().unwrap_or("");
            let shown: String = text.chars().take(80).collect();
            r.log(
                false,
                json!({
                    "editor": args["editor"],
                    "kind": args["kind"],
                    "origin": args["origin"],
                    "changes": changes,
                    "from": first["from"],
                    "to": first["to"],
                    "text": shown,
                    "result": match result { Ok(v) => v.clone(), Err(e) => json!({ "error": e }) },
                }),
            );
        });
        let r = reg.clone();
        ctx.after(EDIT_SAVE, move |_, args, result| {
            let mut e = args.clone();
            e["result"] = match result {
                Ok(v) => v.clone(),
                Err(e) => json!({ "error": e }),
            };
            r.log(true, e);
        });
        let r = reg.clone();
        ctx.collector(
            FnCollector::new(
                "edit_changes",
                "The open editors (lines, version, selection, highlighter, history) and the last `edit.change` runs.",
            )
            .snapshot(move |_| {
                json!({
                    "editors": r.all().iter().map(EditorState::describe).collect::<Vec<_>>(),
                    "focused": r.focused().map(|s| s.id()),
                    "changes": r.changes(),
                })
            }),
        )?;
        let r = reg.clone();
        ctx.collector(
            FnCollector::new(
                "edit_saves",
                "The last `edit.save` runs, with their results.",
            )
            .snapshot(move |_| json!({ "saves": r.saves() })),
        )?;
        let r = reg;
        ctx.collector(
            FnCollector::new(
                "code_diagnostics",
                "Each editor's diagnostics, as the last `code.diagnostics` set them.",
            )
            .snapshot(move |_| {
                let all: serde_json::Map<String, Json> = r
                    .all()
                    .iter()
                    .map(|s| (s.id(), json!(s.diagnostics())))
                    .collect();
                Json::Object(all)
            }),
        )?;
        Ok(())
    }
}

fn surface() -> BindOptions {
    BindOptions::surface(SURFACE)
}

/// The editors' commands.
fn commands(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    const CAT: &str = "Editor";
    let editor = json!({ "editor": "notes (optional: the editor that last had the focus)" });
    ctx.command(
        CommandSpec::new(CMD_FIND, "Find", CAT)
            .description("Opens the search bar; with `query` (and `case_sensitive`, `whole_word`, `regex`), searches and selects the first match after the cursor.")
            .key_with("Ctrl+F", surface())
            .args(json!({ "editor": "notes", "query": "todo", "case_sensitive": false, "whole_word": false, "regex": false })),
        |call| {
            let s = target(call)?;
            if call.args.get("query").is_none() {
                open_find(&s, false);
                return Ok(ok(&s));
            }
            {
                let mut m = s.lock();
                m.find.query = query_of(call.args, &m.find.query);
                m.find.open = true;
                m.refresh_matches();
                if let Some(e) = &m.find.error {
                    return Err(e.clone());
                }
            }
            let at = find_step(&s, true);
            let m = s.lock();
            Ok(json!({
                "editor": m.id,
                "matches": m.find.matches.len(),
                "current": at,
                "first": m.find.matches.iter().take(100).collect::<Vec<_>>(),
            }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_FIND_NEXT, "Find next", CAT)
            .description("Selects the next match of the search.")
            .key_with("F3", surface())
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            let i = find_step(&s, true);
            Ok(json!({ "current": i, "selection": s.selection() }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_FIND_PREVIOUS, "Find previous", CAT)
            .description("Selects the previous match of the search.")
            .key_with("Shift+F3", surface())
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            let i = find_step(&s, false);
            Ok(json!({ "current": i, "selection": s.selection() }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_REPLACE, "Replace", CAT)
            .description("Opens the search bar with replace; with `query` and `replacement`, replaces the match at the cursor (or every match with `all`).")
            .key_with("Ctrl+H", surface())
            .args(json!({ "editor": "notes", "query": "colour", "replacement": "color", "all": true })),
        |call| {
            let s = target(call)?;
            let origin = source_of(call.origin);
            if call.args.get("replacement").is_none() {
                open_find(&s, true);
                return Ok(ok(&s));
            }
            let with = call.args["replacement"].as_str().unwrap_or("").to_string();
            let q = {
                let mut m = s.lock();
                m.find.query = query_of(call.args, &m.find.query);
                m.find.replacement = with.clone();
                m.find.query.clone()
            };
            if call.args.get("all").and_then(Json::as_bool) == Some(true) {
                let n = s.replace_all(Some(call.app), &q, &with, &origin)?;
                return Ok(json!({ "replaced": n, "version": s.version() }));
            }
            let done = replace_one(&s, Some(call.app), &origin)?;
            Ok(json!({ "replaced": usize::from(done), "version": s.version() }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_GOTO_LINE, "Go to line", CAT)
            .description("Opens the go-to-line field; with `line` (from 1), puts the cursor there.")
            .key_with("Ctrl+G", surface())
            .args(json!({ "editor": "notes", "line": 120 })),
        |call| {
            let s = target(call)?;
            match call.args.get("line").and_then(Json::as_u64) {
                Some(l) => {
                    goto(&s, l as usize);
                }
                None => open_goto(&s),
            }
            Ok(ok(&s))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_SAVE, "Save", CAT)
            .description("Saves the editor's text through the `edit.save` step.")
            .key_with("Ctrl+S", surface())
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            let v = s.save(Some(call.app), &source_of(call.origin))?;
            Ok(json!({ "editor": s.id(), "saved": v }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_READ, "Read the text", CAT)
            .description("The editor's text (or lines `from_line` to `to_line`, from 0, end excluded), with its version, selection and state.")
            .hidden()
            .args(json!({ "editor": "notes", "from_line": 0, "to_line": 50 })),
        |call| {
            let s = target(call)?;
            let mut out = s.describe();
            let from = call.args.get("from_line").and_then(Json::as_u64);
            let to = call.args.get("to_line").and_then(Json::as_u64);
            out["text"] = json!(match (from, to) {
                (None, None) => s.text(),
                (f, t) => s.lines(f.unwrap_or(0) as usize, t.map_or(usize::MAX, |t| t as usize)),
            });
            Ok(out)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_WRITE, "Write the text", CAT)
            .description("Changes the text through `edit.change`: the whole `text`; or `text` between `from` and `to` (`{line, col}` from 0); or a list of `changes`. One undo step.")
            .hidden()
            .args(json!({ "editor": "notes", "from": { "line": 0, "col": 0 }, "to": { "line": 0, "col": 0 }, "text": "Hello " })),
        |call| {
            let s = target(call)?;
            let origin = source_of(call.origin);
            let changes: Vec<Change> = if let Some(list) = call.args.get("changes") {
                serde_json::from_value(list.clone()).map_err(|e| format!("`changes`: {e}"))?
            } else {
                let text = call.args.get("text").and_then(Json::as_str).ok_or("`text` is missing")?;
                match (call.args.get("from").and_then(pos), call.args.get("to").and_then(pos)) {
                    (Some(from), to) => vec![Change::new(from, to.unwrap_or(from), text)],
                    (None, _) => {
                        let v = s.set_text(Some(call.app), text, &origin)?;
                        return Ok(json!({ "editor": s.id(), "version": v }));
                    }
                }
            };
            let v = s.edit(Some(call.app), changes, Why::Edit(EditKind::External), None, &origin)?;
            Ok(json!({ "editor": s.id(), "version": v, "selection": s.selection() }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_SELECT, "Select", CAT)
            .description("Selects from `anchor` to `head` (`{line, col}` from 0; `head` alone puts the cursor there).")
            .hidden()
            .args(json!({ "editor": "notes", "anchor": { "line": 0, "col": 0 }, "head": { "line": 2, "col": 0 } })),
        |call| {
            let s = target(call)?;
            let head = call.args.get("head").and_then(pos).ok_or("`head` is missing")?;
            let anchor = call.args.get("anchor").and_then(pos).unwrap_or(head);
            s.set_selection(Selection::new(anchor, head));
            Ok(ok(&s))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_UNDO, "Undo in the editor", CAT)
            .description(
                "Undoes the editor's last edit (its own history, as Ctrl+Z in the editor).",
            )
            .hidden()
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            let v = s.undo(Some(call.app), &source_of(call.origin))?;
            Ok(json!({ "editor": s.id(), "undone": v.is_some(), "version": s.version() }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_REDO, "Redo in the editor", CAT)
            .description("Redoes the editor's last undone edit.")
            .hidden()
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            let v = s.redo(Some(call.app), &source_of(call.origin))?;
            Ok(json!({ "editor": s.id(), "redone": v.is_some(), "version": s.version() }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_FOLD, "Fold", CAT)
            .description("Folds the block at the cursor (or at `line`, from 0).")
            .key_with("Ctrl+Shift+OpenBracket", surface())
            .args(json!({ "editor": "main.rs", "line": 10 })),
        |call| {
            let s = target(call)?;
            let line = line_arg(call, &s);
            let folded = fold_at(&s, line);
            Ok(json!({ "editor": s.id(), "folded": folded }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_UNFOLD, "Unfold", CAT)
            .description("Unfolds the fold at the cursor (or at `line`, from 0).")
            .key_with("Ctrl+Shift+CloseBracket", surface())
            .args(json!({ "editor": "main.rs", "line": 10 })),
        |call| {
            let s = target(call)?;
            let line = line_arg(call, &s);
            Ok(json!({ "editor": s.id(), "unfolded": unfold_at(&s, line) }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_UNFOLD_ALL, "Unfold all", CAT)
            .description("Unfolds every fold.")
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            s.lock().folds.clear();
            Ok(ok(&s))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_BREAKPOINT, "Toggle breakpoint", CAT)
            .description("Sets or clears a breakpoint on the cursor's line (or `line`, from 0).")
            .key_with("F9", surface())
            .args(json!({ "editor": "main.rs", "line": 10 })),
        |call| {
            let s = target(call)?;
            let line = line_arg(call, &s);
            let on = s.toggle_breakpoint(line);
            Ok(json!({ "editor": s.id(), "line": line, "breakpoint": on }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_COMPLETE, "Suggest completions", CAT)
            .description("Asks the assist provider for completions at the cursor.")
            .key_with("Ctrl+Space", surface())
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            crate::assist_ui::complete(&s, Some(call.app));
            Ok(ok(&s))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_ACTIONS, "Code actions", CAT)
            .description("Asks the assist provider for code actions and quick fixes at the cursor.")
            .key_with("Ctrl+Period", surface())
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            crate::assist_ui::code_actions(&s, Some(call.app));
            Ok(ok(&s))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_DEFINITION, "Go to definition", CAT)
            .description("Asks the assist provider where the symbol at the cursor is defined, and goes there.")
            .key_with("F12", surface())
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            crate::assist_ui::definition(&s, Some(call.app));
            Ok(ok(&s))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_HOVER, "Show hover", CAT)
            .description(
                "Asks the assist provider about the symbol at the cursor, as hovering does.",
            )
            .key_with("Ctrl+K Ctrl+I", surface())
            .args(editor.clone()),
        |call| {
            let s = target(call)?;
            let at = s.selection().head;
            crate::assist_ui::hover_at(&s, Some(call.app), at);
            Ok(ok(&s))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_DIAGNOSTICS, "Set the diagnostics", CAT)
            .description("Replaces a code editor's diagnostics through the `code.diagnostics` step.")
            .hidden()
            .args(json!({ "editor": "main.rs", "diagnostics": [{ "line": 3, "columns": [4, 9], "severity": "warning", "message": "unused variable", "source": "rustc" }] })),
        |call| {
            let s = target(call)?;
            let list: Vec<Diagnostic> = serde_json::from_value(call.args["diagnostics"].clone())
                .map_err(|e| format!("`diagnostics`: {e}"))?;
            let n = s.set_diagnostics(Some(call.app), list)?;
            Ok(json!({ "editor": s.id(), "diagnostics": n }))
        },
    )?;
    Ok(())
}

/// Runs a step through the application when there is one, else its
/// default directly.
pub(crate) fn run(
    app: Option<&AppHandle>,
    step: &StepId,
    args: Json,
    default: impl FnOnce(&Json) -> Result<Json, String>,
) -> Outcome {
    match app {
        Some(app) => app.step(step, args, |_, args| default(args)),
        None => Outcome::Done(default(&args)),
    }
}
