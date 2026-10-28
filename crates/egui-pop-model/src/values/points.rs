//! The store's histories beyond undo, redo and jump (ADR 0014): the
//! snapshots pinned in a history, the redo lists new steps dropped
//! (replaced lines, reachable by a jump), undoing one step out of order,
//! and the values at a point, for a preview.

use std::collections::BTreeMap;

use super::{Origin, OwnerId, Refused, Step, Store, Value};
use crate::history::{At, Refusal, RefusalKind, ScopeId, Snapshot, may_undo_out_of_order};

/// A redo list a new step dropped.
#[derive(Clone, Debug)]
pub(super) struct Branch {
    /// The step it came after (0: the start).
    pub(super) after: u64,
    /// Its steps, oldest first.
    pub(super) steps: Vec<Step>,
}

/// A snapshot, with the values it pins.
#[derive(Clone, Debug)]
pub(super) struct Snap {
    pub(super) info: Snapshot,
    /// Each path the history touched, with its value when the snapshot
    /// was taken; a path first changed later is added by that step.
    pub(super) values: BTreeMap<String, Value>,
}

fn paths_of(s: &Step) -> Vec<&String> {
    s.changes.iter().map(|(p, _, _)| p).collect()
}

fn held(r: Refused) -> Refusal {
    let kind = match r {
        Refused::Held { .. } => RefusalKind::Held,
        _ => RefusalKind::Owner,
    };
    Refusal::new(kind, r.to_string())
}

impl Store {
    fn no_gesture(&self, history: &ScopeId) -> Result<(), Refusal> {
        match self.gesture_open(history) {
            Some(label) => Err(Refusal::new(
                RefusalKind::Held,
                format!("“{label}” is in progress"),
            )),
            None => Ok(()),
        }
    }

    /// A history's snapshots, oldest first.
    pub fn snapshots_in(&self, history: &ScopeId) -> Vec<Snapshot> {
        self.histories
            .get(history)
            .map(|h| h.snapshots.iter().map(|s| s.info.clone()).collect())
            .unwrap_or_default()
    }

    /// Pins the history's current state under `name`: the value of every
    /// path its steps touched. Never dropped by the cap. With `dry_run`,
    /// the snapshot it would take (id 0), nothing kept.
    pub fn take_snapshot(
        &mut self,
        history: &ScopeId,
        name: &str,
        origin: &Origin,
        dry_run: bool,
    ) -> Result<Snapshot, Refusal> {
        self.no_gesture(history)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Refusal::new(RefusalKind::Owner, "a snapshot needs a name"));
        }
        let h = self.histories.get(history);
        let step = h.and_then(|h| h.undo.last()).map_or(0, |s| s.id);
        let mut info = Snapshot {
            id: 0,
            scope: history.clone(),
            name: name.into(),
            step,
            origin: origin.clone(),
            at: self.now(),
        };
        if dry_run {
            return Ok(info);
        }
        let mut values = BTreeMap::new();
        if let Some(h) = h {
            let all = h
                .undo
                .iter()
                .chain(&h.redo)
                .chain(h.replaced.iter().flat_map(|b| &b.steps));
            for s in all {
                for (path, _, _) in &s.changes {
                    if let Some(e) = self.entries.get(path) {
                        values.insert(path.clone(), e.value.clone());
                    }
                }
            }
        }
        self.next_snapshot += 1;
        info.id = self.next_snapshot;
        self.histories
            .entry(history.clone())
            .or_default()
            .snapshots
            .push(Snap {
                info: info.clone(),
                values,
            });
        Ok(info)
    }

    /// Deletes a snapshot; `false` when there was none.
    pub fn forget_snapshot(&mut self, history: &ScopeId, id: u64) -> bool {
        let Some(h) = self.histories.get_mut(history) else {
            return false;
        };
        let n = h.snapshots.len();
        h.snapshots.retain(|s| s.info.id != id);
        h.snapshots.len() != n
    }

    fn snap(&self, history: &ScopeId, id: u64) -> Result<&Snap, Refusal> {
        self.histories
            .get(history)
            .and_then(|h| h.snapshots.iter().find(|s| s.info.id == id))
            .ok_or_else(|| {
                Refusal::new(
                    RefusalKind::Nothing,
                    format!("no snapshot {id} in `{history}`"),
                )
            })
    }

    /// Brings the history back to a snapshot's state, as one new step
    /// ("Restore “name”") that an undo takes back. `None`: the state is
    /// already the snapshot's. With `dry_run`, the step it would make
    /// (id 0), nothing changed.
    pub fn restore_snapshot(
        &mut self,
        history: &ScopeId,
        id: u64,
        origin: &Origin,
        dry_run: bool,
    ) -> Result<Option<Step>, Refusal> {
        self.no_gesture(history)?;
        let snap = self.snap(history, id)?;
        let label = format!("Restore “{}”", snap.info.name);
        let changes: Vec<(String, Value, Value)> = snap
            .values
            .iter()
            .filter_map(|(p, v)| {
                let e = self.entries.get(p)?;
                (e.value != *v).then(|| (p.clone(), e.value.clone(), v.clone()))
            })
            .collect();
        if changes.is_empty() {
            return Ok(None);
        }
        self.write_step(history, label, origin, changes, dry_run)
            .map(Some)
    }

    /// Writes `changes` (path, old, new) as one new step of `history`,
    /// checked first; with `dry_run`, only the step it would be (id 0).
    fn write_step(
        &mut self,
        history: &ScopeId,
        label: String,
        origin: &Origin,
        changes: Vec<(String, Value, Value)>,
        dry_run: bool,
    ) -> Result<Step, Refusal> {
        let mut step = Step {
            id: 0,
            label,
            origin: origin.clone(),
            changes,
            at: self.now(),
            place: self.place.clone(),
            merge: None,
            group: None,
        };
        self.can_replay(&step).map_err(held)?;
        if dry_run {
            return Ok(step);
        }
        for (path, _, new) in &step.changes {
            if self.entries[path].value != *new {
                self.write(path, new.clone(), origin, Some(OwnerId::FRAMEWORK));
            }
        }
        self.next_step += 1;
        step.id = self.next_step;
        if let Some(h) = self.histories.get_mut(history) {
            h.sealed = true;
        }
        self.push_step(history, step.clone());
        Ok(step)
    }

    /// Undoes one step out of order (VS Code's rule): the head as an undo
    /// does; an older step by a new step ("Undo “label”") putting back
    /// the values it changed, refused when a later step changed one of
    /// them. The step undone; with `dry_run`, checked only.
    pub fn undo_step_in(
        &mut self,
        history: &ScopeId,
        id: u64,
        origin: &Origin,
        dry_run: bool,
    ) -> Result<Step, Refusal> {
        self.no_gesture(history)?;
        let nothing = |why: String| Refusal::new(RefusalKind::Nothing, why);
        let h = self
            .histories
            .get(history)
            .ok_or_else(|| nothing(format!("no step {id} in `{history}`")))?;
        let Some(i) = h.undo.iter().position(|s| s.id == id) else {
            let why = if h.redo.iter().any(|s| s.id == id) {
                format!("step {id} is undone already")
            } else {
                format!("no step {id} in `{history}`")
            };
            return Err(nothing(why));
        };
        let step = h.undo[i].clone();
        if i + 1 == h.undo.len() {
            if dry_run {
                self.can_replay(&step).map_err(held)?;
                return Ok(step);
            }
            return self
                .undo_in(history, origin)
                .map_err(held)?
                .ok_or_else(|| nothing(format!("no step {id} in `{history}`")));
        }
        if let Err((later, path)) = may_undo_out_of_order(&h.undo, i, paths_of) {
            return Err(Refusal::new(
                RefusalKind::Owner,
                format!(
                    "a later step, “{}”, changed `{path}` too: undo it first",
                    h.undo[later].label
                ),
            ));
        }
        // Back to the values it found, from what they are now.
        let mut changes: Vec<(String, Value, Value)> = Vec::new();
        for (path, old, _) in step.changes.iter().rev() {
            let Some(e) = self.entries.get(path) else {
                return Err(held(Refused::Unknown { path: path.clone() }));
            };
            match changes.iter_mut().find(|(p, _, _)| p == path) {
                Some(c) => c.2 = old.clone(),
                None => changes.push((path.clone(), e.value.clone(), old.clone())),
            }
        }
        changes.retain(|(_, now, then)| now != then);
        self.write_step(
            history,
            format!("Undo “{}”", step.label),
            origin,
            changes,
            dry_run,
        )?;
        Ok(step)
    }

    /// A history's replaced steps, each with the step its line left
    /// after (0: the start), oldest line first, each line oldest first.
    pub fn replaced_in(&self, history: &ScopeId) -> Vec<(u64, &Step)> {
        self.histories
            .get(history)
            .map(|h| {
                h.replaced
                    .iter()
                    .flat_map(|b| b.steps.iter().map(move |s| (b.after, s)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Brings a history to a replaced step: back (or forward) to where its
    /// line left, then along it; the current redo list becomes a
    /// replaced line in its place. All or nothing. The steps walked, in
    /// order; `None` when `to` is not a replaced step, or when its line
    /// left from a step that is itself replaced (go to that line first).
    /// With `dry_run`, checked and answered, nothing changed.
    pub fn jump_replaced(
        &mut self,
        history: &ScopeId,
        to: u64,
        origin: &Origin,
        dry_run: bool,
    ) -> Result<Option<Vec<Step>>, Refused> {
        let Some(h) = self.histories.get(history) else {
            return Ok(None);
        };
        let Some((b, pos)) = h.replaced.iter().enumerate().find_map(|(b, line)| {
            line.steps
                .iter()
                .position(|s| s.id == to)
                .map(|pos| (b, pos))
        }) else {
            return Ok(None);
        };
        let after = h.replaced[b].after;
        let Some((redo, count)) = self.plan_jump(history, after) else {
            return Ok(None);
        };
        let back: Vec<Step> = {
            let list = if redo { &h.redo } else { &h.undo };
            list.iter().rev().take(count).cloned().collect()
        };
        let along: Vec<Step> = h.replaced[b].steps[..=pos].to_vec();
        for s in back.iter().chain(&along) {
            self.can_replay(s)?;
        }
        let walked: Vec<Step> = back.into_iter().chain(along).collect();
        if dry_run {
            return Ok(Some(walked));
        }
        for _ in 0..count {
            self.walk(history, origin, redo)?;
        }
        let h = self.histories.get_mut(history).expect("still there");
        let line = h.replaced.remove(b);
        let old: Vec<Step> = h.redo.drain(..).rev().collect();
        if !old.is_empty() {
            h.replaced.push(Branch { after, steps: old });
        }
        h.redo = line.steps.into_iter().rev().collect();
        h.rev += 1;
        for _ in 0..=pos {
            self.walk(history, origin, true)?;
        }
        Ok(Some(walked))
    }

    /// The values that would differ at a point of a history (a step, 0
    /// the start, a replaced step, or a snapshot): each path with its
    /// value now and its value there. `None`: no such point.
    pub fn values_at(&self, history: &ScopeId, at: At) -> Option<Vec<(String, Value, Value)>> {
        let then = match at {
            At::Snapshot(id) => self.snap(history, id).ok()?.values.clone(),
            At::Step(to) => self.state_at(history, to)?,
        };
        Some(
            then.into_iter()
                .filter_map(|(p, v)| {
                    let e = self.entries.get(&p)?;
                    (e.value != v).then(|| (p, e.value.clone(), v))
                })
                .collect(),
        )
    }

    /// The paths a walk to `to` would write, with the values they would
    /// get.
    fn state_at(&self, history: &ScopeId, to: u64) -> Option<BTreeMap<String, Value>> {
        let mut out = BTreeMap::new();
        if let Some((redo, count)) = self.plan_jump(history, to) {
            let Some(h) = self.histories.get(history) else {
                return Some(out);
            };
            let list = if redo { &h.redo } else { &h.undo };
            for s in list.iter().rev().take(count) {
                if redo {
                    for (p, _, new) in &s.changes {
                        out.insert(p.clone(), new.clone());
                    }
                } else {
                    for (p, old, _) in s.changes.iter().rev() {
                        out.insert(p.clone(), old.clone());
                    }
                }
            }
            return Some(out);
        }
        let h = self.histories.get(history)?;
        let (line, pos) = h.replaced.iter().find_map(|line| {
            line.steps
                .iter()
                .position(|s| s.id == to)
                .map(|pos| (line, pos))
        })?;
        let mut out = self.state_at(history, line.after)?;
        for s in &line.steps[..=pos] {
            for (p, _, new) in &s.changes {
                out.insert(p.clone(), new.clone());
            }
        }
        Some(out)
    }
}
