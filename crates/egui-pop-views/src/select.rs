//! Selection by key, shared by the tree, the list and the table.
//!
//! A selection holds keys, never row numbers: rows come and go, are
//! sorted or moved, and what was selected stays selected. The **cursor**
//! is the row the keyboard is on; the **anchor** is where a range
//! (Shift) starts.
//!
//! ```
//! use egui_pop_views::select::{Mode, Selection};
//!
//! let rows = [10, 20, 30, 40];
//! let mut s = Selection::<u32>::default();
//! s.apply(s.click(Mode::Multiple, rows.len(), &|i| rows[i], 1, egui::Modifiers::NONE));
//! s.apply(s.click(Mode::Multiple, rows.len(), &|i| rows[i], 3, egui::Modifiers::SHIFT));
//! assert_eq!(s.keys(), &[20, 30, 40]);
//! assert_eq!(s.cursor(), Some(&40));
//! ```

use std::collections::HashSet;
use std::hash::Hash;

use egui::Modifiers;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value as Json, json};

/// What a view's keys are: cloned, compared, hashed, and carried in
/// step arguments as JSON.
pub trait Key:
    Clone + Eq + Hash + std::fmt::Debug + Serialize + DeserializeOwned + 'static
{
}

impl<T: Clone + Eq + Hash + std::fmt::Debug + Serialize + DeserializeOwned + 'static> Key for T {}

/// How many rows may be selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Rows are not selectable (the cursor still moves).
    None,
    /// One row at a time.
    #[default]
    Single,
    /// Several rows: Ctrl toggles one, Shift extends a range, Ctrl+A
    /// selects all.
    Multiple,
}

/// The selected keys, the cursor and the anchor.
#[derive(Clone, Debug)]
pub struct Selection<K> {
    keys: Vec<K>,
    set: HashSet<K>,
    cursor: Option<K>,
    anchor: Option<K>,
}

impl<K: Key> PartialEq for Selection<K> {
    fn eq(&self, other: &Self) -> bool {
        self.keys == other.keys && self.cursor == other.cursor && self.anchor == other.anchor
    }
}

impl<K> Default for Selection<K> {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            set: HashSet::new(),
            cursor: None,
            anchor: None,
        }
    }
}

/// A selection a gesture proposes, before its step runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal<K> {
    pub keys: Vec<K>,
    pub cursor: Option<K>,
    pub anchor: Option<K>,
}

impl<K: Key> Proposal<K> {
    /// The step's arguments: `keys` and `cursor`, under `field` naming
    /// the view.
    pub fn args(&self, field: &str, view: &str) -> Json {
        json!({ field: view, "keys": self.keys, "cursor": self.cursor })
    }

    /// The proposal as a (possibly redirected) step's arguments give it.
    pub fn from_args(&self, args: &Json) -> Result<Proposal<K>, String> {
        let keys: Vec<K> =
            serde_json::from_value(args["keys"].clone()).map_err(|e| format!("keys: {e}"))?;
        let cursor: Option<K> =
            serde_json::from_value(args["cursor"].clone()).map_err(|e| format!("cursor: {e}"))?;
        let anchor = if keys == self.keys {
            self.anchor.clone()
        } else {
            cursor.clone()
        };
        Ok(Proposal {
            keys,
            cursor,
            anchor,
        })
    }
}

impl<K: Key> Selection<K> {
    /// The selected keys, in the order they were selected.
    pub fn keys(&self) -> &[K] {
        &self.keys
    }

    pub fn contains(&self, key: &K) -> bool {
        self.set.contains(key)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// The row the keyboard is on.
    pub fn cursor(&self) -> Option<&K> {
        self.cursor.as_ref()
    }

    /// Where a Shift range starts.
    pub fn anchor(&self) -> Option<&K> {
        self.anchor.as_ref()
    }

    /// Selects these keys, the cursor on `cursor`.
    pub fn set(&mut self, keys: impl IntoIterator<Item = K>, cursor: Option<K>) {
        self.keys.clear();
        self.set.clear();
        for k in keys {
            if self.set.insert(k.clone()) {
                self.keys.push(k);
            }
        }
        self.anchor = cursor.clone();
        self.cursor = cursor;
    }

    /// Moves the cursor without changing the selection.
    pub fn set_cursor(&mut self, cursor: Option<K>) {
        self.cursor = cursor;
    }

    pub fn clear(&mut self) {
        self.set(Vec::new(), None);
    }

    /// Keeps the keys `keep` accepts (rows that are gone are dropped);
    /// the cursor and anchor go too when refused. Returns whether
    /// anything was dropped.
    pub fn retain(&mut self, keep: impl Fn(&K) -> bool) -> bool {
        let before = self.keys.len();
        self.keys.retain(|k| keep(k));
        self.set.retain(|k| keep(k));
        if self.cursor.as_ref().is_some_and(|c| !keep(c)) {
            self.cursor = None;
        }
        if self.anchor.as_ref().is_some_and(|a| !keep(a)) {
            self.anchor = None;
        }
        before != self.keys.len()
    }

    /// Applies a proposal.
    pub fn apply(&mut self, p: Proposal<K>) {
        self.set(p.keys, p.cursor);
        self.anchor = p.anchor;
    }

    /// The selection a click on row `index` proposes. `key_at` gives
    /// the key of a row; `len` is how many rows there are.
    pub fn click(
        &self,
        mode: Mode,
        len: usize,
        key_at: &dyn Fn(usize) -> K,
        index: usize,
        modifiers: Modifiers,
    ) -> Proposal<K> {
        let key = key_at(index);
        match mode {
            Mode::None => self.only_cursor(key),
            Mode::Single => self.single(key),
            Mode::Multiple if modifiers.command => {
                let mut keys = self.keys.clone();
                if self.contains(&key) {
                    keys.retain(|k| *k != key);
                } else {
                    keys.push(key.clone());
                }
                Proposal {
                    keys,
                    cursor: Some(key.clone()),
                    anchor: Some(key),
                }
            }
            Mode::Multiple if modifiers.shift => self.range(len, key_at, index),
            Mode::Multiple => self.single(key),
        }
    }

    /// The selection moving the cursor to row `index` with the keyboard
    /// proposes: Shift extends from the anchor, Ctrl moves the cursor
    /// alone.
    pub fn move_to(
        &self,
        mode: Mode,
        len: usize,
        key_at: &dyn Fn(usize) -> K,
        index: usize,
        modifiers: Modifiers,
    ) -> Proposal<K> {
        let key = key_at(index);
        match mode {
            Mode::None => self.only_cursor(key),
            Mode::Multiple if modifiers.shift => self.range(len, key_at, index),
            Mode::Multiple if modifiers.command => self.only_cursor(key),
            _ => self.single(key),
        }
    }

    /// Every row (Ctrl+A).
    pub fn all(&self, len: usize, key_at: &dyn Fn(usize) -> K) -> Proposal<K> {
        Proposal {
            keys: (0..len).map(key_at).collect(),
            cursor: self.cursor.clone(),
            anchor: self.anchor.clone(),
        }
    }

    /// Toggles the cursor's row (Ctrl+Space).
    pub fn toggle_cursor(&self, mode: Mode) -> Option<Proposal<K>> {
        let key = self.cursor.clone()?;
        let mut keys = self.keys.clone();
        match mode {
            Mode::None => return None,
            Mode::Single if self.contains(&key) => keys.clear(),
            Mode::Single => keys = vec![key.clone()],
            Mode::Multiple if self.contains(&key) => keys.retain(|k| *k != key),
            Mode::Multiple => keys.push(key.clone()),
        }
        Some(Proposal {
            keys,
            cursor: Some(key.clone()),
            anchor: Some(key),
        })
    }

    fn only_cursor(&self, key: K) -> Proposal<K> {
        Proposal {
            keys: self.keys.clone(),
            cursor: Some(key),
            anchor: self.anchor.clone(),
        }
    }

    fn single(&self, key: K) -> Proposal<K> {
        Proposal {
            keys: vec![key.clone()],
            cursor: Some(key.clone()),
            anchor: Some(key),
        }
    }

    fn range(&self, len: usize, key_at: &dyn Fn(usize) -> K, index: usize) -> Proposal<K> {
        let from = self
            .anchor
            .as_ref()
            .and_then(|a| (0..len).find(|&i| key_at(i) == *a))
            .unwrap_or(index);
        let (lo, hi) = if from <= index {
            (from, index)
        } else {
            (index, from)
        };
        Proposal {
            keys: (lo..=hi).map(key_at).collect(),
            cursor: Some(key_at(index)),
            anchor: Some(key_at(from)),
        }
    }
}

/// Runs a selection step (`list.select`, `table.select`): the default
/// applies the proposal, or what a before-hook redirected it to.
/// Returns the selection made, or why there was none.
pub(crate) fn run<K: Key>(
    app: Option<&egui_pop_api::AppHandle>,
    step: &egui_pop_model::ids::StepId,
    field: &str,
    view: &str,
    selection: &mut Selection<K>,
    p: Proposal<K>,
) -> Result<Proposal<K>, String> {
    let mut done = p.clone();
    let outcome = crate::steps::run(app, step, p.args(field, view), |args| {
        done = p.from_args(args)?;
        selection.apply(done.clone());
        Ok(json!(done.keys.len()))
    });
    match outcome {
        egui_pop_api::steps::Outcome::Done(Ok(_)) => Ok(done),
        egui_pop_api::steps::Outcome::Done(Err(reason))
        | egui_pop_api::steps::Outcome::Vetoed { reason, .. } => Err(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(i: usize) -> u32 {
        i as u32 * 10
    }

    #[test]
    fn ctrl_toggles_and_shift_extends_from_the_anchor() {
        let mut s = Selection::<u32>::default();
        s.apply(s.click(Mode::Multiple, 10, &rows, 2, Modifiers::NONE));
        s.apply(s.click(Mode::Multiple, 10, &rows, 5, Modifiers::COMMAND));
        assert_eq!(s.keys(), &[20, 50]);
        s.apply(s.click(Mode::Multiple, 10, &rows, 3, Modifiers::SHIFT));
        assert_eq!(s.keys(), &[30, 40, 50]);
        assert_eq!(s.anchor(), Some(&50));
        s.apply(s.click(Mode::Multiple, 10, &rows, 5, Modifiers::COMMAND));
        assert_eq!(s.keys(), &[30, 40]);
    }

    #[test]
    fn single_mode_ignores_modifiers_and_retain_drops_gone_rows() {
        let mut s = Selection::<u32>::default();
        s.apply(s.click(Mode::Single, 10, &rows, 2, Modifiers::NONE));
        s.apply(s.click(Mode::Single, 10, &rows, 4, Modifiers::SHIFT));
        assert_eq!(s.keys(), &[40]);
        assert!(s.retain(|k| *k != 40));
        assert!(s.is_empty());
        assert_eq!(s.cursor(), None);
    }

    #[test]
    fn a_redirect_replaces_the_proposal() {
        let s = Selection::<u32>::default();
        let p = s.click(Mode::Multiple, 10, &rows, 2, Modifiers::NONE);
        let args = p.args("list", "l");
        assert_eq!(args, json!({ "list": "l", "keys": [20], "cursor": 20 }));
        let q = p
            .from_args(&json!({ "keys": [30, 40], "cursor": 40 }))
            .unwrap();
        assert_eq!(q.keys, vec![30, 40]);
        assert_eq!(q.anchor, Some(40));
    }
}
