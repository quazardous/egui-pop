//! The editor's state: the text, the selection, the history, what is
//! pinned to lines, folds, the search; shared by the widget, the
//! application and the commands agents run.
//!
//! [`EditorState`] is a handle (cloned, sent to any thread). Every
//! change of the text goes through [`EditorState::edit`], which runs the
//! `edit.change` step when an application is given: a plugin may refuse
//! it, rewrite it, or follow it.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use egui_pop_api::AppHandle;
use egui_pop_api::steps::Outcome;
use egui_pop_core::lock;
use serde_json::{Value as Json, json};

use crate::buffer::{Buffer, Change, Pos, Span};
use crate::fold::Folds;
use crate::highlight::{self, Highlighter};
use crate::history::{EditKind, Entry, History};
use crate::markers::{Diagnostic, Marks};
use crate::search::{self, Query};
use crate::selection::Selection;
use crate::steps::{CODE_DIAGNOSTICS, EDIT_CHANGE, EDIT_SAVE, run};

/// Why the text changes, as `edit.change` says (`kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// An edit, recorded in the history.
    Edit(EditKind),
    Undo,
    Redo,
}

impl Why {
    pub fn name(self) -> &'static str {
        match self {
            Why::Edit(k) => k.label(),
            Why::Undo => "undo",
            Why::Redo => "redo",
        }
    }
}

/// The search bar's state.
#[derive(Clone, Debug, Default)]
pub(crate) struct Find {
    pub open: bool,
    pub replace: bool,
    pub query: Query,
    pub replacement: String,
    pub matches: Vec<Span>,
    built: Option<(u64, Query)>,
    pub current: Option<usize>,
    pub error: Option<String>,
    /// Focus the query field at the next pass.
    pub focus: bool,
    /// The go-to-line field, when open.
    pub goto: Option<String>,
}

/// What the view asks of the next pass.
#[derive(Clone, Debug, Default)]
pub(crate) struct ViewState {
    /// Scroll the cursor into view.
    pub reveal: bool,
    /// Where Up and Down aim, in points from the line's start.
    pub column_x: Option<f32>,
    /// Take the keyboard focus.
    pub focus: bool,
    /// The longest line's length in bytes, for this version.
    pub longest: Option<(u64, usize)>,
    /// Where the cursor was drawn last (screen), for the assist's popups.
    pub cursor_rect: Option<egui::Rect>,
    /// The selection last published (`code.cursor_moved`).
    pub published: Option<Selection>,
}

/// Everything behind an [`EditorState`].
pub struct Model {
    pub(crate) id: String,
    pub(crate) buffer: Buffer,
    pub(crate) version: u64,
    pub(crate) saved: u64,
    pub(crate) selection: Selection,
    pub(crate) history: History,
    pub(crate) marks: Marks,
    pub(crate) folds: Folds,
    pub(crate) highlighter: Box<dyn Highlighter>,
    pub(crate) language: Option<String>,
    pub(crate) read_only: bool,
    /// Columns per tab, and what one level of indentation inserts.
    pub(crate) tab: usize,
    pub(crate) indent: String,
    pub(crate) find: Find,
    pub(crate) view: ViewState,
    pub(crate) assist: crate::assist_ui::AssistState,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("id", &self.id)
            .field("lines", &self.buffer.line_count())
            .field("version", &self.version)
            .field("selection", &self.selection)
            .field("highlighter", &self.highlighter.name())
            .finish()
    }
}

impl Model {
    /// The matches of the current query, recomputed when the text or the
    /// query changed.
    pub(crate) fn refresh_matches(&mut self) {
        let key = (self.version, self.find.query.clone());
        if self.find.built.as_ref() == Some(&key) {
            return;
        }
        match search::find_all(&self.buffer, &self.find.query) {
            Ok(m) => {
                self.find.matches = m;
                self.find.error = None;
            }
            Err(e) => {
                self.find.matches.clear();
                self.find.error = Some(e);
            }
        }
        self.find.current = self
            .find
            .current
            .filter(|&i| i < self.find.matches.len())
            .or_else(|| search::next_from(&self.find.matches, self.selection.span().start));
        self.find.built = Some(key);
    }

    /// Sets the selection, clamped, and scrolls to it.
    pub(crate) fn select(&mut self, s: Selection) {
        self.selection = Selection::new(self.buffer.clamp(s.anchor), self.buffer.clamp(s.head));
        let tab = self.tab;
        self.folds
            .reveal(&self.buffer, self.selection.head.line, tab);
        self.view.reveal = true;
    }

    /// The longest line's length in bytes (for the scroll width).
    pub(crate) fn longest(&mut self) -> usize {
        if let Some((v, n)) = self.view.longest
            && v == self.version
        {
            return n;
        }
        let n = self
            .buffer
            .lines()
            .iter()
            .map(String::len)
            .max()
            .unwrap_or(0);
        self.view.longest = Some((self.version, n));
        n
    }

    /// Applies changes in order: what `edit.change` does by default.
    fn apply(&mut self, changes: &[Change], why: Why, after: Option<Selection>, time: f64) {
        let before = self.selection;
        let mut forward = Vec::with_capacity(changes.len());
        let mut inverse = Vec::with_capacity(changes.len());
        let mut first = usize::MAX;
        let mut caret = before.head;
        for c in changes {
            let (inv, shift) = self.buffer.apply(c);
            first = first.min(inv.from.line);
            caret = inv.to;
            // As applied: its clamped start, the end of what it removed.
            forward.push(Change {
                from: inv.from,
                to: crate::buffer::end_of(inv.from, &inv.text),
                text: c.text.clone(),
            });
            inverse.push(inv);
            self.marks.shift(&shift);
            self.folds.shift(&shift);
        }
        inverse.reverse();
        self.version += 1;
        if first != usize::MAX {
            self.highlighter.edited(first);
        }
        let after = after.unwrap_or(Selection::caret(caret));
        self.select(after);
        self.view.column_x = None;
        if let Why::Edit(kind) = why {
            self.history.record(Entry {
                kind,
                forward,
                inverse,
                before,
                after: self.selection,
                time,
                open: true,
            });
        }
    }
}

/// An editor's state: a handle to its text, selection, history, marks
/// and search. Cloning it gives another handle to the same editor.
#[derive(Clone)]
pub struct EditorState(Arc<Mutex<Model>>);

impl fmt::Debug for EditorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lock().fmt(f)
    }
}

impl EditorState {
    /// An editor named `id` (in steps, commands and the debug bridge)
    /// over `text`.
    pub fn new(id: impl Into<String>, text: &str) -> Self {
        Self(Arc::new(Mutex::new(Model {
            id: id.into(),
            buffer: Buffer::from_text(text),
            version: 0,
            saved: 0,
            selection: Selection::default(),
            history: History::new(),
            marks: Marks::default(),
            folds: Folds::default(),
            highlighter: Box::new(highlight::Plain),
            language: None,
            read_only: false,
            tab: 4,
            indent: "    ".into(),
            find: Find::default(),
            view: ViewState::default(),
            assist: Default::default(),
        })))
    }

    /// Highlights as `language` (a name, an extension or a file name):
    /// syntect's grammar for it, or none.
    pub fn language(self, language: &str) -> Self {
        {
            let mut m = self.lock();
            m.highlighter = highlight::for_language(language);
            m.language = Some(language.to_string());
        }
        self
    }

    /// Highlights with the application's highlighter.
    pub fn highlighter(self, h: Box<dyn Highlighter>) -> Self {
        self.lock().highlighter = h;
        self
    }

    /// Offers code assist through this provider (a code editor without
    /// one completes with the text's words, `WordCompletion`).
    pub fn assist(self, provider: Box<dyn crate::assist::AssistProvider>) -> Self {
        self.lock().assist.provider = Some(provider);
        self
    }

    /// The assist provider's name, if there is one.
    pub fn assist_name(&self) -> Option<String> {
        self.lock().assist.provider.as_ref().map(|p| p.name())
    }

    /// Refuses edits from the keyboard (commands may still change it).
    pub fn read_only(self, read_only: bool) -> Self {
        self.lock().read_only = read_only;
        self
    }

    /// What one level of indentation inserts (`"\t"`, two spaces...).
    pub fn indent_with(self, unit: &str) -> Self {
        self.lock().indent = unit.to_string();
        self
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Model> {
        lock(&self.0)
    }

    /// Whether two handles are the same editor.
    pub fn same(&self, other: &EditorState) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn id(&self) -> String {
        self.lock().id.clone()
    }

    pub fn text(&self) -> String {
        self.lock().buffer.text()
    }

    /// Lines `from..to` (clamped), joined by `\n`.
    pub fn lines(&self, from: usize, to: usize) -> String {
        let m = self.lock();
        let to = to.min(m.buffer.line_count());
        m.buffer.lines()[from.min(to)..to].join("\n")
    }

    pub fn line_count(&self) -> usize {
        self.lock().buffer.line_count()
    }

    /// Bumped by every change.
    pub fn version(&self) -> u64 {
        self.lock().version
    }

    /// Whether the text changed since it was loaded or last saved.
    pub fn is_dirty(&self) -> bool {
        let m = self.lock();
        m.version != m.saved
    }

    /// Marks the current version as saved.
    pub fn mark_saved(&self) {
        let mut m = self.lock();
        m.saved = m.version;
    }

    /// Replaces the whole text without a step or a history entry (a file
    /// loaded): the history is cleared, the text is clean.
    pub fn load(&self, text: &str) {
        let mut m = self.lock();
        m.buffer = Buffer::from_text(text);
        m.version += 1;
        m.saved = m.version;
        m.history.clear();
        m.marks = Marks::default();
        m.folds.clear();
        m.selection = Selection::default();
        m.highlighter.edited(0);
        m.view.reveal = true;
    }

    pub fn selection(&self) -> Selection {
        self.lock().selection
    }

    /// The selected text.
    pub fn selected_text(&self) -> String {
        let m = self.lock();
        m.buffer.slice(m.selection.span())
    }

    /// Selects, and scrolls to the cursor.
    pub fn set_selection(&self, s: Selection) {
        let mut m = self.lock();
        m.history.seal();
        m.select(s);
    }

    /// Asks the editor for the keyboard focus at its next pass.
    pub fn request_focus(&self) {
        self.lock().view.focus = true;
    }

    /// Changes the text through the `edit.change` step (when `app` is
    /// given): the new version, or why it was refused.
    pub fn edit(
        &self,
        app: Option<&AppHandle>,
        changes: Vec<Change>,
        why: Why,
        after: Option<Selection>,
        origin: &str,
    ) -> Result<u64, String> {
        let args = {
            let m = self.lock();
            json!({
                "editor": m.id,
                "kind": why.name(),
                "origin": origin,
                "version": m.version,
                "changes": changes,
                "selection": after,
            })
        };
        let time = app.map_or(0.0, |a| a.egui().input(|i| i.time));
        let me = self.clone();
        let outcome = run(app, &EDIT_CHANGE, args, move |args| {
            let changes: Vec<Change> = serde_json::from_value(args["changes"].clone())
                .map_err(|e| format!("`changes`: {e}"))?;
            let after: Option<Selection> =
                serde_json::from_value(args["selection"].clone()).unwrap_or(None);
            let mut m = me.lock();
            m.apply(&changes, why, after, time);
            Ok(json!({ "version": m.version }))
        });
        if let Some(app) = app {
            app.egui().request_repaint();
        }
        result(outcome).map(|v| v["version"].as_u64().unwrap_or(0))
    }

    /// Replaces the selection with `text` (typing, a paste).
    pub fn insert(
        &self,
        app: Option<&AppHandle>,
        text: &str,
        kind: EditKind,
        origin: &str,
    ) -> Result<u64, String> {
        let span = self.lock().selection.span();
        self.edit(
            app,
            vec![Change::new(span.start, span.end, text)],
            Why::Edit(kind),
            None,
            origin,
        )
    }

    /// Replaces the whole text through `edit.change` (one history entry).
    pub fn set_text(
        &self,
        app: Option<&AppHandle>,
        text: &str,
        origin: &str,
    ) -> Result<u64, String> {
        let end = self.lock().buffer.end();
        self.edit(
            app,
            vec![Change::new(Pos::default(), end, text)],
            Why::Edit(EditKind::External),
            Some(Selection::default()),
            origin,
        )
    }

    pub fn can_undo(&self) -> bool {
        self.lock().history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.lock().history.can_redo()
    }

    /// What an undo would take back ("typing").
    pub fn undo_label(&self) -> Option<&'static str> {
        self.lock().history.undo_label()
    }

    /// Undoes the last entry of the editor's history, through
    /// `edit.change` (kind `undo`); `Ok(None)` when there was nothing.
    pub fn undo(&self, app: Option<&AppHandle>, origin: &str) -> Result<Option<u64>, String> {
        let Some(e) = self.lock().history.pop_undo() else {
            return Ok(None);
        };
        match self.edit(app, e.inverse, Why::Undo, Some(e.before), origin) {
            Ok(v) => Ok(Some(v)),
            Err(r) => {
                self.lock().history.unpop_undo();
                Err(r)
            }
        }
    }

    /// Redoes the last undone entry, through `edit.change` (kind `redo`).
    pub fn redo(&self, app: Option<&AppHandle>, origin: &str) -> Result<Option<u64>, String> {
        let Some(e) = self.lock().history.pop_redo() else {
            return Ok(None);
        };
        match self.edit(app, e.forward, Why::Redo, Some(e.after), origin) {
            Ok(v) => Ok(Some(v)),
            Err(r) => {
                self.lock().history.unpop_redo();
                Err(r)
            }
        }
    }

    /// Saves through the `edit.save` step: the application writes the
    /// text (a run-hook or an after-hook); once it says it is done, the
    /// text is clean. Without an application, it is marked saved.
    pub fn save(&self, app: Option<&AppHandle>, origin: &str) -> Result<u64, String> {
        let (args, version) = {
            let m = self.lock();
            (
                json!({
                    "editor": m.id,
                    "version": m.version,
                    "lines": m.buffer.line_count(),
                    "language": m.language,
                    "origin": origin,
                }),
                m.version,
            )
        };
        let outcome = run(app, &EDIT_SAVE, args, |_| Ok(json!({ "saved": version })));
        result(outcome)?;
        let mut m = self.lock();
        m.saved = version;
        Ok(version)
    }

    /// The matches of a query.
    pub fn find(&self, query: &Query) -> Result<Vec<Span>, String> {
        search::find_all(&self.lock().buffer, query)
    }

    /// Replaces every match through one `edit.change` (one history
    /// entry): how many were replaced.
    pub fn replace_all(
        &self,
        app: Option<&AppHandle>,
        query: &Query,
        with: &str,
        origin: &str,
    ) -> Result<usize, String> {
        let changes: Vec<Change> = {
            let m = self.lock();
            let matches = search::find_all(&m.buffer, query)?;
            // Last first, so earlier positions stay valid.
            matches
                .iter()
                .rev()
                .map(|s| {
                    Change::new(
                        s.start,
                        s.end,
                        search::replacement_for(&m.buffer, query, *s, with),
                    )
                })
                .collect()
        };
        let n = changes.len();
        if n > 0 {
            let sel = self.selection();
            self.edit(
                app,
                changes,
                Why::Edit(EditKind::Replace),
                Some(sel),
                origin,
            )?;
        }
        Ok(n)
    }

    /// Sets the diagnostics through the `code.diagnostics` step (they
    /// replace the previous ones).
    pub fn set_diagnostics(
        &self,
        app: Option<&AppHandle>,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<usize, String> {
        let args = {
            let m = self.lock();
            json!({ "editor": m.id, "version": m.version, "diagnostics": diagnostics })
        };
        let me = self.clone();
        let outcome = run(app, &CODE_DIAGNOSTICS, args, move |args| {
            let list: Vec<Diagnostic> = serde_json::from_value(args["diagnostics"].clone())
                .map_err(|e| format!("`diagnostics`: {e}"))?;
            let n = list.len();
            let mut m = me.lock();
            m.marks.diagnostics = list;
            m.marks.diagnostics.sort_by_key(|d| d.line);
            Ok(json!({ "count": n }))
        });
        if let Some(app) = app {
            app.egui().request_repaint();
        }
        result(outcome).map(|v| v["count"].as_u64().unwrap_or(0) as usize)
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.lock().marks.diagnostics.clone()
    }

    /// The lines with a breakpoint.
    pub fn breakpoints(&self) -> Vec<usize> {
        self.lock().marks.breakpoints.clone()
    }

    pub fn set_breakpoints(&self, lines: Vec<usize>) {
        let mut m = self.lock();
        m.marks.breakpoints = lines;
        m.marks.breakpoints.sort_unstable();
        m.marks.breakpoints.dedup();
    }

    /// Sets or clears a breakpoint; whether it is set now.
    pub fn toggle_breakpoint(&self, line: usize) -> bool {
        self.lock().marks.toggle_breakpoint(line)
    }

    /// Marks lines as added, changed, or followed by removed lines.
    pub fn set_diff(&self, diff: Vec<(usize, crate::markers::Diff)>) {
        self.lock().marks.diff = diff;
    }

    /// Folds the lines under `line`; whether it folds.
    pub fn fold(&self, line: usize) -> bool {
        let mut m = self.lock();
        let tab = m.tab;
        let Model { folds, buffer, .. } = &mut *m;
        let done = folds.fold(buffer, line, tab);
        // The cursor never hides: it goes to the fold's line.
        if done && m.selection.head.line > line {
            let end = crate::fold::fold_end(&m.buffer, line, tab).unwrap_or(line);
            if m.selection.head.line <= end {
                let at = Pos::new(line, m.buffer.line_len(line));
                m.selection = Selection::caret(at);
            }
        }
        done
    }

    pub fn unfold(&self, line: usize) -> bool {
        self.lock().folds.unfold(line)
    }

    /// The folded lines.
    pub fn folded(&self) -> Vec<usize> {
        self.lock().folds.starts().collect()
    }

    /// What the highlighter is (`syntect: Rust`).
    pub fn highlighter_name(&self) -> String {
        self.lock().highlighter.name()
    }

    /// The editor as the debug bridge shows it.
    pub fn describe(&self) -> Json {
        let m = self.lock();
        json!({
            "editor": m.id,
            "lines": m.buffer.line_count(),
            "bytes": m.buffer.byte_len(),
            "version": m.version,
            "dirty": m.version != m.saved,
            "selection": m.selection,
            "language": m.language,
            "highlighter": m.highlighter.name(),
            "read_only": m.read_only,
            "undo": m.history.undo_label(),
            "redo": m.history.redo_label(),
            "diagnostics": m.marks.diagnostics.len(),
            "breakpoints": m.marks.breakpoints,
            "folded": m.folds.starts().collect::<Vec<_>>(),
            "assist": m.assist.provider.as_ref().map(|p| p.name()),
            "completion": crate::assist_ui::shown_labels_of(&m),
            "find": if m.find.open { json!(m.find.query) } else { Json::Null },
        })
    }
}

/// A step's outcome as a result.
fn result(outcome: Outcome) -> Result<Json, String> {
    match outcome {
        Outcome::Done(r) => r,
        Outcome::Vetoed { reason, .. } => Err(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_undo_and_redo_without_an_application() {
        let s = EditorState::new("t", "hello");
        s.set_selection(Selection::caret(Pos::new(0, 5)));
        s.insert(None, " world", EditKind::Paste, "user").unwrap();
        assert_eq!(s.text(), "hello world");
        assert!(s.is_dirty());
        assert_eq!(s.selection(), Selection::caret(Pos::new(0, 11)));
        s.undo(None, "user").unwrap();
        assert_eq!(s.text(), "hello");
        assert_eq!(s.selection(), Selection::caret(Pos::new(0, 5)));
        s.redo(None, "user").unwrap();
        assert_eq!(s.text(), "hello world");
        assert_eq!(
            s.replace_all(None, &Query::new("o"), "0", "user").unwrap(),
            2
        );
        assert_eq!(s.text(), "hell0 w0rld");
        s.undo(None, "user").unwrap();
        assert_eq!(s.text(), "hello world");
        s.save(None, "user").unwrap();
        assert!(!s.is_dirty());
    }

    #[test]
    fn marks_and_folds_follow_edits() {
        let s = EditorState::new("t", "a\n  b\n  c\nd");
        s.toggle_breakpoint(3);
        s.set_selection(Selection::caret(Pos::new(0, 0)));
        s.insert(None, "x\n", EditKind::Typing, "user").unwrap();
        assert_eq!(s.breakpoints(), vec![4]);
        assert!(s.fold(1));
        assert_eq!(s.folded(), vec![1]);
    }
}
