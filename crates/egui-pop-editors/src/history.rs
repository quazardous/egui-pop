//! The editor's own history of edits, for undo and redo.
//!
//! egui-pop's host contract (`Host::undo`) undoes the application's
//! history, and offers no way to add entries to it: the editor keeps its
//! own. Typing in a row coalesces into one entry (until a pause, a word
//! break or a move); every other edit is an entry of its own.

use serde::Serialize;

use crate::buffer::Change;
use crate::selection::Selection;

/// What an entry did, as the history names it ("Undo typing").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EditKind {
    Typing,
    Deletion,
    Paste,
    Cut,
    Indent,
    Replace,
    /// A completion or an inline suggestion accepted.
    Completion,
    /// An edit from the application or an agent.
    External,
}

impl EditKind {
    pub fn label(self) -> &'static str {
        match self {
            EditKind::Typing => "typing",
            EditKind::Deletion => "deletion",
            EditKind::Paste => "paste",
            EditKind::Cut => "cut",
            EditKind::Indent => "indentation",
            EditKind::Replace => "replace",
            EditKind::Completion => "completion",
            EditKind::External => "edit",
        }
    }
}

/// One entry: the changes as applied, their inverses (to apply in this
/// order), and the selection before and after.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub kind: EditKind,
    pub forward: Vec<Change>,
    pub inverse: Vec<Change>,
    pub before: Selection,
    pub after: Selection,
    /// When it was last extended (seconds, egui's input time).
    pub time: f64,
    /// Whether the next typing may join it.
    pub open: bool,
}

/// How long a pause ends a run of typing, in seconds.
pub const COALESCE: f64 = 1.0;

/// The undo and redo stacks.
#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    /// At most this many entries (0: no limit).
    pub limit: usize,
}

impl History {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: 1000,
        }
    }

    /// Records an applied edit, joining the last entry when both are
    /// typing close in time and place.
    pub fn record(&mut self, mut entry: Entry) {
        self.redo.clear();
        if entry.kind == EditKind::Typing
            && let Some(last) = self.undo.last_mut()
            && last.open
            && last.kind == EditKind::Typing
            && entry.time - last.time < COALESCE
            && last.after == entry.before
            && !breaks_word(&entry.forward)
        {
            last.forward.append(&mut entry.forward);
            let mut inv = entry.inverse;
            inv.append(&mut last.inverse);
            last.inverse = inv;
            last.after = entry.after;
            last.time = entry.time;
            return;
        }
        entry.open = entry.kind == EditKind::Typing;
        self.undo.push(entry);
        if self.limit > 0 && self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }

    /// Ends the current run of typing (a move, a click).
    pub fn seal(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.open = false;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What an undo would take back.
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|e| e.kind.label())
    }

    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(|e| e.kind.label())
    }

    /// The entry an undo applies the inverse of (moved to redo).
    pub fn pop_undo(&mut self) -> Option<Entry> {
        let e = self.undo.pop()?;
        self.redo.push(e.clone());
        Some(e)
    }

    /// The entry a redo applies again (moved back to undo).
    pub fn pop_redo(&mut self) -> Option<Entry> {
        let mut e = self.redo.pop()?;
        e.open = false;
        self.undo.push(e.clone());
        Some(e)
    }

    /// Puts back an entry whose undo was refused.
    pub fn unpop_undo(&mut self) {
        if let Some(e) = self.redo.pop() {
            self.undo.push(e);
        }
    }

    /// Puts back an entry whose redo was refused.
    pub fn unpop_redo(&mut self) {
        if let Some(e) = self.undo.pop() {
            self.redo.push(e);
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// How many entries can be undone.
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }
}

/// Typing a space or a line break starts a new entry after it.
fn breaks_word(changes: &[Change]) -> bool {
    changes
        .iter()
        .any(|c| c.text.starts_with(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Pos;

    fn typed(at: usize, ch: &str, time: f64) -> Entry {
        let p = Pos::new(0, at);
        let q = Pos::new(0, at + 1);
        Entry {
            kind: EditKind::Typing,
            forward: vec![Change::insert(p, ch)],
            inverse: vec![Change::new(p, q, "")],
            before: Selection::caret(p),
            after: Selection::caret(q),
            time,
            open: true,
        }
    }

    #[test]
    fn typing_coalesces_until_a_pause_or_a_space() {
        let mut h = History::new();
        h.record(typed(0, "a", 0.0));
        h.record(typed(1, "b", 0.2));
        assert_eq!(h.len(), 1);
        h.record(typed(2, " ", 0.3));
        assert_eq!(h.len(), 2);
        h.record(typed(3, "c", 5.0));
        assert_eq!(h.len(), 3);
        h.seal();
        h.record(typed(4, "d", 5.1));
        assert_eq!(h.len(), 4);
        let e = h.pop_undo().unwrap();
        assert_eq!(e.inverse.len(), 1);
        assert!(h.can_redo());
        h.unpop_undo();
        assert!(!h.can_redo());
    }
}
