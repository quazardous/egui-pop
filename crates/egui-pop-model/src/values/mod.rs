//! The value store: every value a widget, a setting or an agent edits
//! lives here at a stable path (`settings/theme`, `view/zoom`), never in a
//! widget.
//!
//! - Each path has a [`Descriptor`]: type, hard and soft range, fine and
//!   coarse steps, unit, display scale and precision, default, label,
//!   choices.
//! - Every write is a [`Set`]: an absolute value, a relative step, reset,
//!   top or bottom, with its [`Origin`]. The answer says what was applied
//!   and whether it was clamped; a refusal says why and gives the current
//!   value.
//! - During a gesture (a drag), the paths it touched belong to it: a write
//!   from another origin is refused. Otherwise the last writer wins; a
//!   writer may ask to write only if nothing changed since it read
//!   (`if_seq`).
//! - Every change is told to the listeners of its path and kept in a
//!   short journal ([`Changed`]: path, old, new, origin, sequence); a write
//!   equal to the value changes nothing and tells nothing, so a change
//!   never loops back.
//! - Undo: one step per gesture or group, else one per write.
//! - Paths are owned: a plugin's settings and listeners go with it.
//!
//! ```
//! use egui_pop_model::commands::Origin;
//! use egui_pop_model::ids::OwnerId;
//! use egui_pop_model::values::{Change, Descriptor, Set, Store, Value};
//!
//! let mut store = Store::new();
//! store
//!     .declare(&OwnerId::FRAMEWORK, "settings/volume", Descriptor::float("Volume", 0.0, 1.0, 0.5))
//!     .unwrap();
//! let applied = store
//!     .set(Set::new("settings/volume", Change::To(Value::Float(2.0)), Origin::User))
//!     .unwrap();
//! assert_eq!(applied.value, Value::Float(1.0)); // clamped to the range
//! ```

mod commands;
mod value;

pub use commands::{register, register_with};
pub use value::{Descriptor, PathHint, PathKind, Type, Value};

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::commands::Origin;
use crate::diagnostics::Diagnostics;
use crate::doc::{Schema, same};
use crate::ids::{OwnerId, check};

/// A value at its path.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    pub descriptor: Descriptor,
    pub value: Value,
    /// The store's sequence number when it last changed (0: never).
    pub seq: u64,
    /// Who changed it last; `None` until someone does.
    pub origin: Option<Origin>,
    /// The order it was declared in.
    pub order: u64,
    /// Who declared it.
    pub owner: OwnerId,
}

/// What a write asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    To(Value),
    /// So many fine steps (negative: down).
    Step(i64),
    /// So many coarse steps.
    Coarse(i64),
    Reset,
    Top,
    Bottom,
}

/// A write.
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    pub path: String,
    pub change: Change,
    pub origin: Origin,
    /// Past the soft range, up to the hard one.
    pub force: bool,
    pub gesture: Option<GestureId>,
    /// Only if the value has not changed since this sequence number.
    pub if_seq: Option<u64>,
    /// Not an undo step (state the application keeps for itself, such as
    /// the window's place).
    pub untracked: bool,
}

impl Set {
    pub fn new(path: &str, change: Change, origin: Origin) -> Self {
        Self {
            path: path.into(),
            change,
            origin,
            force: false,
            gesture: None,
            if_seq: None,
            untracked: false,
        }
    }

    /// Leaves no undo step.
    pub fn untracked(mut self) -> Self {
        self.untracked = true;
        self
    }

    pub fn force(mut self) -> Self {
        self.force = true;
        self
    }

    pub fn gesture(mut self, gesture: GestureId) -> Self {
        self.gesture = Some(gesture);
        self
    }

    pub fn if_seq(mut self, seq: u64) -> Self {
        self.if_seq = Some(seq);
        self
    }
}

/// Which range held a write back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Clamped {
    None,
    Soft,
    Hard,
}

/// What a write did.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Applied {
    pub value: Value,
    pub clamped: Clamped,
    /// The value's sequence number now.
    pub seq: u64,
    /// False when the value was already that.
    pub changed: bool,
}

/// Why a write was refused; each gives the current value.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "refused", rename_all = "snake_case")]
pub enum Refused {
    Unknown {
        path: String,
    },
    Invalid {
        reason: String,
        current: Value,
    },
    /// It changed since the writer read it.
    Stale {
        current: Value,
        seq: u64,
    },
    /// Someone else's gesture holds it.
    Held {
        current: Value,
        by: Origin,
    },
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::Unknown { path } => write!(f, "no value at `{path}`"),
            Refused::Invalid { reason, current } => {
                write!(f, "{reason} (the value stays {current:?})")
            }
            Refused::Stale { current, seq } => write!(
                f,
                "it changed since it was read (now {current:?}, sequence {seq})"
            ),
            Refused::Held { current, by } => write!(
                f,
                "a gesture of {by:?} holds it (the value stays {current:?})"
            ),
        }
    }
}

impl std::error::Error for Refused {}

/// Why a path could not be declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclareError {
    BadPath {
        path: String,
        reason: &'static str,
    },
    /// Another owner declared it.
    Taken {
        path: String,
        by: OwnerId,
    },
    /// The default does not fit the descriptor.
    BadDefault {
        path: String,
        reason: String,
    },
}

impl fmt::Display for DeclareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeclareError::BadPath { path, reason } => write!(f, "`{path}` is not a path: {reason}"),
            DeclareError::Taken { path, by } => write!(f, "`{path}` is already declared by `{by}`"),
            DeclareError::BadDefault { path, reason } => {
                write!(f, "`{path}`: its default does not fit: {reason}")
            }
        }
    }
}

impl std::error::Error for DeclareError {}

/// A value changed.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Changed {
    pub path: String,
    pub old: Value,
    pub new: Value,
    pub origin: Origin,
    pub seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct GestureId(pub u64);

/// A change listener, by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ListenerId(pub u64);

type Listener = Box<dyn FnMut(&Changed) + Send>;

struct ListenerSlot {
    owner: OwnerId,
    prefix: String,
    f: Listener,
}

#[derive(Debug)]
struct Gesture {
    origin: Origin,
    label: String,
    /// Whether the paths it touched are held against other origins: a
    /// drag holds them, a group of an agent's writes does not.
    holds: bool,
    paths: BTreeSet<String>,
    changes: Vec<(String, Value, Value)>,
}

/// One undo step.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Step {
    pub label: String,
    pub origin: Origin,
    /// Path, old value, new value, in the order written.
    pub changes: Vec<(String, Value, Value)>,
}

/// How many changes the journal keeps.
pub const JOURNAL: usize = 1024;

/// The store.
#[derive(Default)]
pub struct Store {
    entries: BTreeMap<String, Entry>,
    seq: u64,
    next_order: u64,
    gestures: BTreeMap<GestureId, Gesture>,
    next_gesture: u64,
    undo: Vec<Step>,
    redo: Vec<Step>,
    listeners: BTreeMap<ListenerId, ListenerSlot>,
    next_listener: u64,
    journal: VecDeque<Changed>,
    /// Saved values for paths not declared yet (a plugin not connected).
    pending: BTreeMap<String, Json>,
}

impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Store")
            .field("paths", &self.entries.len())
            .field("seq", &self.seq)
            .field("listeners", &self.listeners.len())
            .field("gestures", &self.gestures)
            .finish()
    }
}

fn clamp(v: f64, range: Option<(f64, f64)>, soft: bool) -> (f64, Clamped) {
    match range {
        Some((lo, hi)) if v < lo || v > hi => (
            v.clamp(lo, hi),
            if soft { Clamped::Soft } else { Clamped::Hard },
        ),
        _ => (v, Clamped::None),
    }
}

/// Checks a path: segments with the id syntax, separated by `/`.
pub fn check_path(path: &str) -> Result<(), &'static str> {
    check(path)?;
    if path.split('/').any(str::is_empty) {
        return Err("it has an empty segment");
    }
    Ok(())
}

/// The `settings` document: the values that differ from their default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsDoc {
    pub values: BTreeMap<String, Json>,
}

/// The `settings` document's schema.
pub fn settings_schema() -> Schema {
    Schema::new("settings", 1).migration(0, same)
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares a path at its default value. The same owner may declare a
    /// path again (its descriptor is replaced, its value kept when it
    /// still fits); another owner may not.
    pub fn declare(
        &mut self,
        owner: &OwnerId,
        path: &str,
        descriptor: Descriptor,
    ) -> Result<(), DeclareError> {
        let value = descriptor.default.clone();
        self.declare_with(owner, path, descriptor, value)
    }

    /// Declares a path at a value.
    pub fn declare_with(
        &mut self,
        owner: &OwnerId,
        path: &str,
        descriptor: Descriptor,
        value: Value,
    ) -> Result<(), DeclareError> {
        check_path(path).map_err(|reason| DeclareError::BadPath {
            path: path.into(),
            reason,
        })?;
        let probe = Entry {
            descriptor: descriptor.clone(),
            value: descriptor.default.clone(),
            seq: 0,
            origin: None,
            order: 0,
            owner: owner.clone(),
        };
        let default = Set::new(path, Change::To(descriptor.default.clone()), Origin::User).force();
        match Self::target(&probe, &default) {
            Ok((_, Clamped::None)) => {}
            Ok(_) => {
                return Err(DeclareError::BadDefault {
                    path: path.into(),
                    reason: "it is out of range".into(),
                });
            }
            Err(reason) => {
                return Err(DeclareError::BadDefault {
                    path: path.into(),
                    reason,
                });
            }
        }
        let (order, mut value) = match self.entries.get(path) {
            Some(e) if e.owner != *owner => {
                return Err(DeclareError::Taken {
                    path: path.into(),
                    by: e.owner.clone(),
                });
            }
            Some(e) => (e.order, e.value.clone()),
            None => {
                self.next_order += 1;
                (self.next_order, value)
            }
        };
        let mut entry = Entry {
            descriptor,
            value: value.clone(),
            seq: 0,
            origin: None,
            order,
            owner: owner.clone(),
        };
        // A saved value waiting for this path, or the kept one: only if
        // it fits.
        if let Some(saved) = self.pending.remove(path)
            && let Ok(v) = Value::from_json(entry.descriptor.ty, &saved)
        {
            value = v;
        }
        let check = Set::new(path, Change::To(value), Origin::User).force();
        entry.value = Self::target(&entry, &check)
            .map(|(v, _)| v)
            .unwrap_or_else(|_| entry.descriptor.default.clone());
        self.entries.insert(path.into(), entry);
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.entries.get(path)
    }

    /// The paths under a prefix (`settings/`), by path.
    pub fn list(&self, prefix: &str) -> Vec<(&str, &Entry)> {
        self.entries
            .range(prefix.to_string()..)
            .take_while(|(p, _)| p.starts_with(prefix))
            .map(|(p, e)| (p.as_str(), e))
            .collect()
    }

    /// The paths under a prefix, in declaration order.
    pub fn list_in_order(&self, prefix: &str) -> Vec<(&str, &Entry)> {
        let mut v = self.list(prefix);
        v.sort_by_key(|(_, e)| e.order);
        v
    }

    /// The store's sequence number: the last change's.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// What a change makes of the value, before ranges are checked.
    fn target(entry: &Entry, set: &Set) -> Result<(Value, Clamped), String> {
        let d = &entry.descriptor;
        let range = d.range(set.force);
        let soft = !set.force && d.soft.is_some();
        let number = |v: f64| -> Result<(Value, Clamped), String> {
            if !v.is_finite() {
                return Err(format!("{v} is not a number"));
            }
            let (v, c) = clamp(v, range, soft);
            Ok(match d.ty {
                Type::Int => (Value::Int(v.round() as i64), c),
                _ => (Value::Float(v), c),
            })
        };
        let current = entry.value.as_f64();
        match (&set.change, d.ty) {
            (Change::Reset, _) => Ok((d.default.clone(), Clamped::None)),
            (Change::To(Value::Float(v)), Type::Float | Type::Int) => number(*v),
            (Change::To(Value::Int(v)), Type::Float | Type::Int) => number(*v as f64),
            (Change::To(Value::Floats(v)), Type::Floats) => {
                if let Some(n) = d.count
                    && v.len() > n as usize
                {
                    return Err(format!("{} values, at most {n}", v.len()));
                }
                let mut clamped = Clamped::None;
                let mut out = Vec::with_capacity(v.len());
                for x in v {
                    if !x.is_finite() {
                        return Err(format!("{x} is not a number"));
                    }
                    let (x, c) = clamp(*x, d.hard, false);
                    if c != Clamped::None {
                        clamped = c;
                    }
                    out.push(x);
                }
                Ok((Value::Floats(out), clamped))
            }
            (Change::To(Value::Ints(v)), Type::Ints) => {
                if let Some(n) = d.count
                    && v.len() > n as usize
                {
                    return Err(format!("{} values, at most {n}", v.len()));
                }
                let mut clamped = Clamped::None;
                let out = v
                    .iter()
                    .map(|x| {
                        let (x, c) = clamp(*x as f64, d.hard, false);
                        if c != Clamped::None {
                            clamped = c;
                        }
                        x as i64
                    })
                    .collect();
                Ok((Value::Ints(out), clamped))
            }
            (Change::To(Value::Bool(b)), Type::Bool) => Ok((Value::Bool(*b), Clamped::None)),
            (Change::To(Value::Choice(c)), Type::Choice) => {
                if d.choices.contains(c) {
                    Ok((Value::Choice(c.clone()), Clamped::None))
                } else {
                    Err(format!("`{c}` is not one of {}", d.choices.join(", ")))
                }
            }
            (Change::To(Value::Text(t)), Type::Text) => match d.capacity {
                Some(n) if t.len() > n as usize => Err(format!("{} bytes, at most {n}", t.len())),
                _ => Ok((Value::Text(t.clone()), Clamped::None)),
            },
            (Change::To(v), _) => Err(format!("{v:?} is not a {:?}", d.ty)),
            (Change::Step(n) | Change::Coarse(n), Type::Float | Type::Int) => {
                let step = if matches!(set.change, Change::Coarse(_)) {
                    d.coarse
                } else {
                    d.step
                };
                number(current.unwrap_or(0.0) + *n as f64 * step)
            }
            (Change::Step(n) | Change::Coarse(n), Type::Bool) => {
                let b = entry.value.as_bool().unwrap_or(false);
                Ok((Value::Bool(b ^ (n % 2 != 0)), Clamped::None))
            }
            (Change::Step(n) | Change::Coarse(n), Type::Choice) => {
                if d.choices.is_empty() {
                    return Err("no choices to step through".into());
                }
                let at = entry
                    .value
                    .as_choice()
                    .and_then(|c| d.choices.iter().position(|o| o == c))
                    .unwrap_or(0) as i64;
                let last = d.choices.len() as i64 - 1;
                let to = at + n;
                let clamped = if to < 0 || to > last {
                    Clamped::Hard
                } else {
                    Clamped::None
                };
                Ok((
                    Value::Choice(d.choices[to.clamp(0, last) as usize].clone()),
                    clamped,
                ))
            }
            (Change::Top | Change::Bottom, Type::Float | Type::Int) => {
                let (lo, hi) = range.ok_or("no range to go to the end of")?;
                number(if set.change == Change::Top { hi } else { lo })
            }
            (Change::Top | Change::Bottom, Type::Bool) => {
                Ok((Value::Bool(set.change == Change::Top), Clamped::None))
            }
            (Change::Top | Change::Bottom, Type::Choice) => {
                let c = if set.change == Change::Top {
                    d.choices.last()
                } else {
                    d.choices.first()
                };
                Ok((Value::Choice(c.cloned().unwrap_or_default()), Clamped::None))
            }
            (_, ty) => Err(format!("a {ty:?} cannot be stepped")),
        }
    }

    /// The origin of the gesture holding a path, if one does: a widget
    /// shows it, an agent knows to wait.
    pub fn held_by(&self, path: &str) -> Option<&Origin> {
        self.gestures
            .values()
            .find(|g| g.holds && g.paths.contains(path))
            .map(|g| &g.origin)
    }

    /// Who holds a path against `origin`, if anyone.
    fn holder(&self, path: &str, origin: &Origin, gesture: Option<GestureId>) -> Option<&Origin> {
        self.gestures
            .iter()
            .find(|(id, g)| {
                g.holds && Some(**id) != gesture && g.origin != *origin && g.paths.contains(path)
            })
            .map(|(_, g)| &g.origin)
    }

    /// Writes a value already checked; tells the listeners.
    fn write(&mut self, path: &str, value: Value, origin: &Origin) -> (Value, u64) {
        self.seq += 1;
        let seq = self.seq;
        let entry = self.entries.get_mut(path).expect("a declared path");
        let old = std::mem::replace(&mut entry.value, value.clone());
        entry.seq = seq;
        entry.origin = Some(origin.clone());
        let event = Changed {
            path: path.into(),
            old: old.clone(),
            new: value,
            origin: origin.clone(),
            seq,
        };
        for l in self.listeners.values_mut() {
            if path.starts_with(&l.prefix) {
                (l.f)(&event);
            }
        }
        if self.journal.len() == JOURNAL {
            self.journal.pop_front();
        }
        self.journal.push_back(event);
        (old, seq)
    }

    /// Applies a write.
    pub fn set(&mut self, set: Set) -> Result<Applied, Refused> {
        let entry = self
            .entries
            .get(&set.path)
            .ok_or_else(|| Refused::Unknown {
                path: set.path.clone(),
            })?;
        let current = entry.value.clone();
        if let Some(seq) = set.if_seq
            && entry.seq != seq
        {
            return Err(Refused::Stale {
                current,
                seq: entry.seq,
            });
        }
        if let Some(by) = self.holder(&set.path, &set.origin, set.gesture) {
            return Err(Refused::Held {
                current,
                by: by.clone(),
            });
        }
        if let Some(g) = set.gesture
            && !self.gestures.contains_key(&g)
        {
            return Err(Refused::Invalid {
                reason: format!("no gesture {}", g.0),
                current,
            });
        }
        let (value, clamped) = Self::target(entry, &set).map_err(|reason| Refused::Invalid {
            reason,
            current: current.clone(),
        })?;
        if value == current {
            return Ok(Applied {
                value,
                clamped,
                seq: entry.seq,
                changed: false,
            });
        }
        let (old, seq) = self.write(&set.path, value.clone(), &set.origin);
        let change = (set.path.clone(), old, value.clone());
        match set.gesture.and_then(|g| self.gestures.get_mut(&g)) {
            _ if set.untracked => {}
            Some(g) => {
                g.paths.insert(set.path.clone());
                g.changes.push(change);
            }
            None => self.push_step(Step {
                label: format!("set {}", set.path),
                origin: set.origin,
                changes: vec![change],
            }),
        }
        Ok(Applied {
            value,
            clamped,
            seq,
            changed: true,
        })
    }

    fn push_step(&mut self, step: Step) {
        self.undo.push(step);
        self.redo.clear();
    }

    fn open(&mut self, origin: Origin, label: &str, holds: bool) -> GestureId {
        self.next_gesture += 1;
        let id = GestureId(self.next_gesture);
        self.gestures.insert(
            id,
            Gesture {
                origin,
                label: label.into(),
                holds,
                paths: BTreeSet::new(),
                changes: Vec::new(),
            },
        );
        id
    }

    /// Starts a gesture (a drag): its writes are one undo step, and the
    /// paths it touches are held against other origins until it ends.
    pub fn begin_gesture(&mut self, origin: Origin, label: &str) -> GestureId {
        self.open(origin, label, true)
    }

    /// Starts a group: its writes are one undo step, nothing is held.
    pub fn begin_group(&mut self, origin: Origin, label: &str) -> GestureId {
        self.open(origin, label, false)
    }

    /// Ends a gesture or a group; what it changed becomes one undo step.
    /// Ending one that is not open is reported as `false`.
    pub fn end(&mut self, id: GestureId) -> bool {
        let Some(g) = self.gestures.remove(&id) else {
            return false;
        };
        if !g.changes.is_empty() {
            self.push_step(Step {
                label: g.label,
                origin: g.origin,
                changes: g.changes,
            });
        }
        true
    }

    /// Cancels a gesture or a group: what it changed is put back as it
    /// was (listeners are told), and no undo step is left. Cancelling one
    /// that is not open is reported as `false`.
    pub fn cancel(&mut self, id: GestureId) -> bool {
        let Some(g) = self.gestures.remove(&id) else {
            return false;
        };
        for (path, old, _) in g.changes.into_iter().rev() {
            if self.entries.contains_key(&path) {
                self.write(&path, old, &g.origin);
            }
        }
        true
    }

    /// The undo steps, oldest first.
    pub fn undo_steps(&self) -> &[Step] {
        &self.undo
    }

    /// The redo steps, the next one last.
    pub fn redo_steps(&self) -> &[Step] {
        &self.redo
    }

    /// Puts back the values a step changed, as a write by `origin`.
    fn replay(&mut self, step: &Step, origin: &Origin, back: bool) -> Result<(), Refused> {
        let changes: Vec<(String, Value)> = if back {
            step.changes
                .iter()
                .rev()
                .map(|(p, old, _)| (p.clone(), old.clone()))
                .collect()
        } else {
            step.changes
                .iter()
                .map(|(p, _, new)| (p.clone(), new.clone()))
                .collect()
        };
        for (path, _) in &changes {
            let current = self
                .entries
                .get(path)
                .map(|e| e.value.clone())
                .ok_or_else(|| Refused::Unknown { path: path.clone() })?;
            if let Some(by) = self.holder(path, origin, None) {
                return Err(Refused::Held {
                    current,
                    by: by.clone(),
                });
            }
        }
        for (path, value) in changes {
            if self.entries[&path].value != value {
                self.write(&path, value, origin);
            }
        }
        Ok(())
    }

    /// Undoes the last step; its label, or `None` when there is none.
    pub fn undo(&mut self, origin: &Origin) -> Result<Option<String>, Refused> {
        let Some(step) = self.undo.pop() else {
            return Ok(None);
        };
        if let Err(e) = self.replay(&step, origin, true) {
            self.undo.push(step);
            return Err(e);
        }
        let label = step.label.clone();
        self.redo.push(step);
        Ok(Some(label))
    }

    /// Redoes the last undone step.
    pub fn redo(&mut self, origin: &Origin) -> Result<Option<String>, Refused> {
        let Some(step) = self.redo.pop() else {
            return Ok(None);
        };
        if let Err(e) = self.replay(&step, origin, false) {
            self.redo.push(step);
            return Err(e);
        }
        let label = step.label.clone();
        self.undo.push(step);
        Ok(Some(label))
    }

    /// Calls `f` after every change of a path starting with `prefix`
    /// (empty: every path), until removed.
    pub fn listen(
        &mut self,
        owner: &OwnerId,
        prefix: &str,
        f: impl FnMut(&Changed) + Send + 'static,
    ) -> ListenerId {
        self.next_listener += 1;
        let id = ListenerId(self.next_listener);
        self.listeners.insert(
            id,
            ListenerSlot {
                owner: owner.clone(),
                prefix: prefix.into(),
                f: Box::new(f),
            },
        );
        id
    }

    /// Removes a listener; `false` when it was not there.
    pub fn unlisten(&mut self, id: ListenerId) -> bool {
        self.listeners.remove(&id).is_some()
    }

    /// The listeners, by id: owner and prefix.
    pub fn listeners(&self) -> Vec<(ListenerId, &OwnerId, &str)> {
        self.listeners
            .iter()
            .map(|(id, l)| (*id, &l.owner, l.prefix.as_str()))
            .collect()
    }

    /// The changes after a sequence number still in the journal.
    pub fn changes_since(&self, seq: u64) -> Vec<&Changed> {
        self.journal.iter().filter(|c| c.seq > seq).collect()
    }

    /// Removes an owner's paths and listeners; their undo steps go too.
    /// Changed values of removed paths are kept as pending, so they come
    /// back when the owner declares them again.
    pub fn remove_owner(&mut self, owner: &OwnerId) -> Vec<String> {
        let paths: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.owner == *owner)
            .map(|(p, _)| p.clone())
            .collect();
        for p in &paths {
            if let Some(e) = self.entries.remove(p)
                && e.value != e.descriptor.default
            {
                self.pending.insert(p.clone(), e.value.to_json());
            }
        }
        let gone = |s: &Step| s.changes.iter().any(|(p, _, _)| paths.contains(p));
        self.undo.retain(|s| !gone(s));
        self.redo.retain(|s| !gone(s));
        self.listeners.retain(|_, l| l.owner != *owner);
        paths
    }

    /// The values under a prefix that differ from their default, and the
    /// saved values still waiting for their path: what the `settings`
    /// document holds.
    pub fn export(&self, prefix: &str) -> SettingsDoc {
        let mut values: BTreeMap<String, Json> = self
            .pending
            .iter()
            .filter(|(p, _)| p.starts_with(prefix))
            .map(|(p, v)| (p.clone(), v.clone()))
            .collect();
        for (p, e) in self.list(prefix) {
            if e.value != e.descriptor.default {
                values.insert(p.into(), e.value.to_json());
            }
        }
        SettingsDoc { values }
    }

    /// Applies saved values as writes by `origin` (no undo step). A value
    /// for a path not declared yet waits for it; one that does not fit its
    /// path is skipped and reported.
    pub fn import(&mut self, doc: &SettingsDoc, origin: &Origin) -> Diagnostics {
        let mut d = Diagnostics::new();
        for (path, json) in &doc.values {
            let Some(e) = self.entries.get(path) else {
                self.pending.insert(path.clone(), json.clone());
                continue;
            };
            let result = Value::from_json(e.descriptor.ty, json).and_then(|v| {
                let set = Set::new(path, Change::To(v), origin.clone()).force();
                Self::target(e, &set).map(|(v, _)| v)
            });
            match result {
                Ok(v) => {
                    if self.entries[path].value != v {
                        self.write(path, v, origin);
                    }
                }
                Err(reason) => d.warn(
                    "bad-setting",
                    None,
                    path,
                    format!("{reason}: the saved value is skipped, the current one stays"),
                ),
            }
        }
        d
    }

    /// Saved values waiting for their path to be declared.
    pub fn pending(&self) -> &BTreeMap<String, Json> {
        &self.pending
    }
}
