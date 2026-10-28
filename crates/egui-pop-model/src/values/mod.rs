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
//! - Undo: one step per gesture or group, else one per write; the newest
//!   [`HISTORY_CAP`] steps are kept unless the host sets another cap. An
//!   undo or a redo never writes into a drag still going, the dragger's
//!   own included. Steps go to a history by scope (ADR 0014): the
//!   application's ([`APP`]) unless the write names another
//!   ([`Set::history`], or the one [`enter_history`] set, such as the
//!   settings window's). Each step has an id, its time on the store's
//!   clock ([`Store::set_clock`]) and the place it was made in
//!   ([`Store::set_place`]); writes carrying the same merge key
//!   ([`Set::merge`]) from one origin less than a second apart are one
//!   step, until [`Store::seal`] (the focus left).
//! - A history also keeps its snapshots ([`Store::take_snapshot`],
//!   restored as a new step), the redo lists new steps dropped
//!   ([`Store::replaced_in`], reachable by [`Store::jump_in`]), and
//!   undoes one step out of order unless a later step changed the same
//!   path ([`Store::undo_step_in`]); [`Store::values_at`] says what
//!   differs at a point, for a preview.
//! - Paths are owned: a plugin's settings and listeners go with it.
//! - One writer per value: a write names the scope issuing it
//!   ([`Set::by`], or the scope [`enter_scope`] set, such as the plugin
//!   whose command runs). A scope that is not the path's owner, not one
//!   of its [`Writers`] and not the framework (acting for a person: the
//!   settings form, a bound widget, an agent's `value.set`) is refused,
//!   or only warned about under [`WriterCheck::Warn`]. A write naming no
//!   scope is the application's own and is not checked; undo, redo and
//!   restoring saved values are never refused.
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
mod points;
mod value;

pub use commands::{register, register_with};
pub use value::{Descriptor, PathHint, PathKind, Type, Value, Writers};

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::clock::SharedClock;
use crate::commands::Origin;
use crate::context::Entry as Place;
use crate::diagnostics::Diagnostics;
use crate::doc::{Schema, same};
pub use crate::history::{APP, SETTINGS};
use crate::history::{MergeMark, ScopeId, merges};
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
    /// The scope of the last write that changed it, when it named one.
    pub writer: Option<OwnerId>,
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
    /// The scope issuing it (a plugin, the plugin whose command runs, the
    /// framework); `None`: the one [`enter_scope`] set, else the
    /// application's own code, not checked.
    pub by: Option<OwnerId>,
    /// Repeats with the same key are one undo step while they come less
    /// than a second apart ([`crate::history::merges`]).
    pub merge: Option<String>,
    /// The history its step goes to; `None`: the one [`enter_history`]
    /// set, else [`APP`].
    pub history: Option<ScopeId>,
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
            by: None,
            merge: None,
            history: None,
        }
    }

    /// Merged with the previous step when it carries the same key, comes
    /// from the same origin less than a second before, and the history
    /// was not sealed since (a held arrow, a wheel's notches).
    pub fn merge(mut self, key: &str) -> Self {
        self.merge = Some(key.into());
        self
    }

    /// Its step goes to this history.
    pub fn history(mut self, scope: &ScopeId) -> Self {
        self.history = Some(scope.clone());
        self
    }

    /// Issued by this scope (checked against the path's writers).
    pub fn by(mut self, scope: &OwnerId) -> Self {
        self.by = Some(scope.clone());
        self
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
    /// The scope issuing it is not one of the path's writers.
    NotWriter {
        path: String,
        by: OwnerId,
        owner: OwnerId,
        current: Value,
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
            Refused::NotWriter {
                path,
                by,
                owner,
                current,
            } => write!(
                f,
                "`{by}` may not write `{path}`, owned by `{owner}` (the value stays {current:?})"
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
    /// The history its step goes to.
    history: ScopeId,
    /// Whether the paths it touched are held against other origins: a
    /// drag holds them, a group of an agent's writes does not.
    holds: bool,
    paths: BTreeSet<String>,
    changes: Vec<(String, Value, Value)>,
}

/// One undo step.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Step {
    /// Unique in the store, from 1.
    pub id: u64,
    pub label: String,
    pub origin: Origin,
    /// Path, old value, new value, in the order written.
    pub changes: Vec<(String, Value, Value)>,
    /// When it was made (its last merged write), on the store's clock.
    pub at: f64,
    /// The place of the context stack it was made in.
    pub place: Vec<Place>,
    /// Its merge key, when its writes merge with the next.
    pub merge: Option<String>,
    /// The gesture or group it was made of.
    pub group: Option<u64>,
}

/// One history of the store: its undo and redo steps.
#[derive(Clone, Debug, Default)]
struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Undo and redo steps the cap dropped so far.
    dropped: (u64, u64),
    /// Moves on with every change of either list.
    rev: u64,
    /// The next step does not merge with the head.
    sealed: bool,
    /// Redo lists new steps dropped, oldest first: shown as "replaced"
    /// steps, reachable by a jump.
    replaced: Vec<points::Branch>,
    /// Named states pinned in it, oldest first.
    snapshots: Vec<points::Snap>,
}

/// How many changes the journal keeps.
pub const JOURNAL: usize = 1024;

/// What a write from a scope that is not one of a path's writers gets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WriterCheck {
    /// Refused, with a diagnostic (the default).
    #[default]
    Refuse,
    /// Applied, with a warning: to migrate plugins written before the
    /// check.
    Warn,
}

/// The diagnostic code of a write from a scope that is not a writer.
pub const NOT_A_WRITER: &str = "not-a-writer";

thread_local! {
    static SCOPES: RefCell<Vec<OwnerId>> = const { RefCell::new(Vec::new()) };
    static HISTORIES: RefCell<Vec<ScopeId>> = const { RefCell::new(Vec::new()) };
}

/// Steps made on this thread go to `scope`'s history while the guard
/// lives, unless a write names its own ([`Set::history`]); a gesture or a
/// group keeps the history it was opened in. The settings window enters
/// [`SETTINGS`] while it draws its form. Histories nest.
pub fn enter_history(scope: &ScopeId) -> HistoryGuard {
    HISTORIES.with(|s| s.borrow_mut().push(scope.clone()));
    HistoryGuard {
        _not_send: std::marker::PhantomData,
    }
}

/// The history [`enter_history`] set on this thread, else [`APP`].
pub fn current_history() -> ScopeId {
    HISTORIES
        .with(|s| s.borrow().last().cloned())
        .unwrap_or(APP)
}

/// Leaves the history [`enter_history`] entered when dropped.
#[must_use = "the history is left when the guard is dropped"]
#[derive(Debug)]
pub struct HistoryGuard {
    _not_send: std::marker::PhantomData<*const ()>,
}

impl Drop for HistoryGuard {
    fn drop(&mut self) {
        HISTORIES.with(|s| s.borrow_mut().pop());
    }
}

/// Writes on this thread that name no scope are issued by `scope` while
/// the guard lives (the application sets the plugin connecting, the
/// owner of the command running, the plugin a lifecycle hook belongs
/// to). Scopes nest.
pub fn enter_scope(scope: &OwnerId) -> ScopeGuard {
    SCOPES.with(|s| s.borrow_mut().push(scope.clone()));
    ScopeGuard {
        _not_send: std::marker::PhantomData,
    }
}

/// The scope [`enter_scope`] set on this thread, if any.
pub fn current_scope() -> Option<OwnerId> {
    SCOPES.with(|s| s.borrow().last().cloned())
}

/// Leaves the scope [`enter_scope`] entered when dropped.
#[must_use = "the scope is left when the guard is dropped"]
#[derive(Debug)]
pub struct ScopeGuard {
    _not_send: std::marker::PhantomData<*const ()>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        SCOPES.with(|s| s.borrow_mut().pop());
    }
}

/// A write from a scope that is not one of a path's writers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StrayWrite {
    pub path: String,
    pub by: OwnerId,
    pub owner: OwnerId,
    /// Whether it was refused (else applied, under [`WriterCheck::Warn`]).
    pub refused: bool,
}

/// How many undo steps a store keeps by default, the oldest dropped
/// first ([`Store::set_history_cap`]).
pub const HISTORY_CAP: usize = 500;

/// The store.
#[derive(Default)]
pub struct Store {
    entries: BTreeMap<String, Entry>,
    seq: u64,
    next_order: u64,
    gestures: BTreeMap<GestureId, Gesture>,
    next_gesture: u64,
    /// The histories, by scope ([`APP`] first made).
    histories: BTreeMap<ScopeId, History>,
    next_step: u64,
    next_snapshot: u64,
    /// The host's cap on each history; [`HISTORY_CAP`] when unset.
    history_cap: Option<usize>,
    /// Where steps take their time from; none: 0, and nothing merges.
    clock: Option<SharedClock>,
    /// The place new steps are made in.
    place: Vec<Place>,
    listeners: BTreeMap<ListenerId, ListenerSlot>,
    next_listener: u64,
    journal: VecDeque<Changed>,
    /// Saved values for paths not declared yet (a plugin not connected).
    pending: BTreeMap<String, Json>,
    check: WriterCheck,
    /// Writes from scopes that are not writers, each path and scope once.
    stray: Vec<StrayWrite>,
    diagnostics: Diagnostics,
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

/// The `settings` document's schema. Version 2 reads a retired pointer
/// profile id as the profile it names (`adobe` as `lightroom`).
pub fn settings_schema() -> Schema {
    Schema::new("settings", 2)
        .migration(0, same)
        .migration(1, crate::pointer::settings_profile_aliases)
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
            writer: None,
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
            writer: None,
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

    /// What a write from a scope that is not a writer gets: refused or
    /// warned about, as [`WriterCheck`] says; each path and scope is
    /// reported once.
    pub fn set_writer_check(&mut self, check: WriterCheck) {
        self.check = check;
    }

    pub fn writer_check(&self) -> WriterCheck {
        self.check
    }

    /// The writes from scopes that are not writers so far, each path and
    /// scope once: what a test of a plugin set expects empty.
    pub fn stray_writes(&self) -> &[StrayWrite] {
        &self.stray
    }

    /// The store's diagnostics: writes from scopes that are not writers.
    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    /// Whether a scope may write a path (an unknown path: no).
    pub fn may_write(&self, path: &str, scope: &OwnerId) -> bool {
        self.entries
            .get(path)
            .is_some_and(|e| e.descriptor.writers.allow(&e.owner, scope))
    }

    /// Checks a write's scope; `Err` when it is refused.
    fn check_writer(&mut self, path: &str, scope: Option<&OwnerId>) -> Result<(), Refused> {
        let (Some(scope), Some(e)) = (scope, self.entries.get(path)) else {
            return Ok(());
        };
        if e.descriptor.writers.allow(&e.owner, scope) {
            return Ok(());
        }
        let refused = self.check == WriterCheck::Refuse;
        let (owner, current) = (e.owner.clone(), e.value.clone());
        if !self
            .stray
            .iter()
            .any(|s| s.path == path && s.by == *scope && s.refused == refused)
        {
            self.stray.push(StrayWrite {
                path: path.into(),
                by: scope.clone(),
                owner: owner.clone(),
                refused,
            });
            let message = format!(
                "`{scope}` is not one of its writers (its owner is `{owner}`): {}",
                if refused {
                    "the write is refused; declare the writer (`Descriptor::writers`)"
                } else {
                    "the write is applied (the check only warns)"
                }
            );
            if refused {
                self.diagnostics
                    .error(NOT_A_WRITER, Some(scope), path, message);
            } else {
                self.diagnostics
                    .warn(NOT_A_WRITER, Some(scope), path, message);
            }
        }
        if refused {
            return Err(Refused::NotWriter {
                path: path.into(),
                by: scope.clone(),
                owner,
                current,
            });
        }
        Ok(())
    }

    /// Writes a value already checked; tells the listeners.
    fn write(
        &mut self,
        path: &str,
        value: Value,
        origin: &Origin,
        by: Option<OwnerId>,
    ) -> (Value, u64) {
        self.seq += 1;
        let seq = self.seq;
        let entry = self.entries.get_mut(path).expect("a declared path");
        let old = std::mem::replace(&mut entry.value, value.clone());
        entry.seq = seq;
        entry.origin = Some(origin.clone());
        entry.writer = by;
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

    /// Applies a write. Its scope ([`Set::by`], else [`current_scope`])
    /// must be one of the path's writers: its owner, one its descriptor
    /// names ([`Writers`]), or the framework; a write naming none is not
    /// checked.
    pub fn set(&mut self, set: Set) -> Result<Applied, Refused> {
        if !self.entries.contains_key(&set.path) {
            return Err(Refused::Unknown {
                path: set.path.clone(),
            });
        }
        let scope = set.by.clone().or_else(current_scope);
        self.check_writer(&set.path, scope.as_ref())?;
        let entry = &self.entries[&set.path];
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
        let (old, seq) = self.write(&set.path, value.clone(), &set.origin, scope);
        let change = (set.path.clone(), old, value.clone());
        match set.gesture.and_then(|g| self.gestures.get_mut(&g)) {
            _ if set.untracked => {}
            Some(g) => {
                g.paths.insert(set.path.clone());
                g.changes.push(change);
            }
            None => {
                let history = set.history.clone().unwrap_or_else(current_history);
                let step = self.new_step(
                    format!("set {}", set.path),
                    set.origin,
                    vec![change],
                    set.merge.clone(),
                    None,
                );
                self.push_step(&history, step);
            }
        }
        Ok(Applied {
            value,
            clamped,
            seq,
            changed: true,
        })
    }

    fn new_step(
        &mut self,
        label: String,
        origin: Origin,
        changes: Vec<(String, Value, Value)>,
        merge: Option<String>,
        group: Option<u64>,
    ) -> Step {
        self.next_step += 1;
        Step {
            id: self.next_step,
            label,
            origin,
            changes,
            at: self.now(),
            place: self.place.clone(),
            merge,
            group,
        }
    }

    /// The store's clock now (0 without one).
    pub fn now(&self) -> f64 {
        self.clock.as_ref().map_or(0.0, |c| c.now())
    }

    /// Where steps take their time from (the interface's clock); without
    /// one, steps are made at 0 and writes never merge.
    pub fn set_clock(&mut self, clock: Option<SharedClock>) {
        self.clock = clock;
    }

    /// Whether the store has a clock.
    pub fn has_clock(&self) -> bool {
        self.clock.is_some()
    }

    /// The place of the context stack new steps are made in (the
    /// interface follows the stack), root first.
    pub fn set_place(&mut self, place: Vec<Place>) {
        self.place = place;
    }

    /// The next step of every history starts a new one, even with the
    /// head's merge key (the focus left the target).
    pub fn seal(&mut self) {
        for h in self.histories.values_mut() {
            h.sealed = true;
        }
    }

    fn push_step(&mut self, history: &ScopeId, step: Step) {
        let clocked = self.clock.is_some();
        let h = self.histories.entry(history.clone()).or_default();
        h.rev += 1;
        // A snapshot keeps the value each path had when it was taken: a
        // path changed for the first time since is recorded now.
        for snap in &mut h.snapshots {
            for (path, old, _) in &step.changes {
                snap.values
                    .entry(path.clone())
                    .or_insert_with(|| old.clone());
            }
        }
        let joins = clocked && !h.sealed && h.redo.is_empty();
        let head = h.undo.last_mut().filter(|_| joins);
        if let (Some(head), Some(key)) = (head, &step.merge)
            && let Some(prev) = &head.merge
            && merges(
                &MergeMark::new(prev, head.origin.clone(), head.at),
                &MergeMark::new(key, step.origin.clone(), step.at),
            )
        {
            for (path, old, new) in step.changes {
                match head.changes.iter_mut().find(|(p, _, _)| *p == path) {
                    Some(c) => c.2 = new,
                    None => head.changes.push((path, old, new)),
                }
            }
            head.at = step.at;
            head.label = step.label;
            return;
        }
        h.sealed = false;
        // The redo list a new step drops is kept, as a replaced line.
        let after = h.undo.last().map_or(0, |s| s.id);
        let dropped: Vec<Step> = h.redo.drain(..).rev().collect();
        if !dropped.is_empty() {
            h.replaced.push(points::Branch {
                after,
                steps: dropped,
            });
        }
        h.undo.push(step);
        self.trim_history();
    }

    /// How many undo steps the store keeps: [`HISTORY_CAP`] unless the
    /// host set another cap.
    pub fn history_cap(&self) -> usize {
        self.history_cap.unwrap_or(HISTORY_CAP)
    }

    /// Keeps at most `cap` undo steps (and as many redo steps), dropping
    /// the oldest now and whenever a new step goes over it. 0 keeps none.
    pub fn set_history_cap(&mut self, cap: usize) {
        self.history_cap = Some(cap);
        self.trim_history();
    }

    fn trim_history(&mut self) {
        let cap = self.history_cap();
        for h in self.histories.values_mut() {
            for (steps, dropped) in [
                (&mut h.undo, &mut h.dropped.0),
                (&mut h.redo, &mut h.dropped.1),
            ] {
                // The oldest undo step and the farthest redo step come first.
                let over = steps.len().saturating_sub(cap);
                if over > 0 {
                    steps.drain(..over);
                    *dropped += over as u64;
                    h.rev += 1;
                }
            }
            // Replaced lines: the oldest go first.
            while h.replaced.iter().map(|b| b.steps.len()).sum::<usize>() > cap {
                h.replaced.remove(0);
                h.rev += 1;
            }
        }
    }

    /// How many undo steps and redo steps the cap dropped from the
    /// application's history ([`APP`]) since the store began, each from
    /// the front of its list.
    pub fn dropped_steps(&self) -> (u64, u64) {
        self.dropped_steps_in(&APP)
    }

    /// How many steps the cap dropped from a history.
    pub fn dropped_steps_in(&self, history: &ScopeId) -> (u64, u64) {
        self.histories.get(history).map_or((0, 0), |h| h.dropped)
    }

    /// The histories that have had a step, by scope.
    pub fn histories(&self) -> Vec<ScopeId> {
        self.histories.keys().cloned().collect()
    }

    /// A history's revision: it moves on with every change of its lists.
    pub fn history_rev(&self, history: &ScopeId) -> u64 {
        self.histories.get(history).map_or(0, |h| h.rev)
    }

    fn open(&mut self, origin: Origin, label: &str, holds: bool) -> GestureId {
        self.next_gesture += 1;
        let id = GestureId(self.next_gesture);
        self.gestures.insert(
            id,
            Gesture {
                origin,
                label: label.into(),
                history: current_history(),
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
            let step = self.new_step(g.label, g.origin, g.changes, None, Some(id.0));
            self.push_step(&g.history, step);
        }
        true
    }

    /// The label of a gesture still going (a drag) whose writes would go
    /// to `history`: an undo there waits for it.
    pub fn gesture_open(&self, history: &ScopeId) -> Option<&str> {
        self.gestures
            .values()
            .find(|g| g.holds && g.history == *history)
            .map(|g| g.label.as_str())
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
                self.write(&path, old, &g.origin, Some(OwnerId::FRAMEWORK));
            }
        }
        true
    }

    /// The application's undo steps ([`APP`]), oldest first.
    pub fn undo_steps(&self) -> &[Step] {
        self.undo_steps_in(&APP)
    }

    /// The application's redo steps, the next one last.
    pub fn redo_steps(&self) -> &[Step] {
        self.redo_steps_in(&APP)
    }

    /// A history's undo steps, oldest first.
    pub fn undo_steps_in(&self, history: &ScopeId) -> &[Step] {
        self.histories.get(history).map_or(&[], |h| &h.undo)
    }

    /// A history's redo steps, the next one last.
    pub fn redo_steps_in(&self, history: &ScopeId) -> &[Step] {
        self.histories.get(history).map_or(&[], |h| &h.redo)
    }

    /// Whether a step can be put back (`back`) or made again: its paths
    /// exist and no drag still going holds one.
    fn can_replay(&self, step: &Step) -> Result<(), Refused> {
        for (path, _, _) in &step.changes {
            let current = self
                .entries
                .get(path)
                .map(|e| e.value.clone())
                .ok_or_else(|| Refused::Unknown { path: path.clone() })?;
            // A drag still going holds its paths against every undo and
            // redo, the dragger's own included: its step is not made yet.
            if let Some(by) = self.held_by(path) {
                return Err(Refused::Held {
                    current,
                    by: by.clone(),
                });
            }
        }
        Ok(())
    }

    /// Puts back the values a step changed, as a write by `origin`.
    fn replay(&mut self, step: &Step, origin: &Origin, back: bool) -> Result<(), Refused> {
        self.can_replay(step)?;
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
        for (path, value) in changes {
            if self.entries[&path].value != value {
                self.write(&path, value, origin, Some(OwnerId::FRAMEWORK));
            }
        }
        Ok(())
    }

    /// Undoes the application's last step ([`APP`]); its label, or
    /// `None` when there is none.
    pub fn undo(&mut self, origin: &Origin) -> Result<Option<String>, Refused> {
        Ok(self.undo_in(&APP, origin)?.map(|s| s.label))
    }

    /// Redoes the application's last undone step.
    pub fn redo(&mut self, origin: &Origin) -> Result<Option<String>, Refused> {
        Ok(self.redo_in(&APP, origin)?.map(|s| s.label))
    }

    /// Undoes a history's last step; the step, or `None` when there is
    /// none.
    pub fn undo_in(&mut self, history: &ScopeId, origin: &Origin) -> Result<Option<Step>, Refused> {
        self.walk(history, origin, false)
    }

    /// Redoes a history's last undone step.
    pub fn redo_in(&mut self, history: &ScopeId, origin: &Origin) -> Result<Option<Step>, Refused> {
        self.walk(history, origin, true)
    }

    /// Whether an undo (or a redo) of a history would be refused, without
    /// making it: the step it would take, or `None` when there is none.
    pub fn check_walk(&self, history: &ScopeId, redo: bool) -> Result<Option<&Step>, Refused> {
        let Some(h) = self.histories.get(history) else {
            return Ok(None);
        };
        let Some(step) = (if redo { h.redo.last() } else { h.undo.last() }) else {
            return Ok(None);
        };
        self.can_replay(step)?;
        Ok(Some(step))
    }

    /// Brings a history to step `to` (0: everything undone) by undos or
    /// redos, all or none: every step on the way is checked first. How
    /// many steps it walked, or `None` when `to` is not in the history.
    pub fn jump_in(
        &mut self,
        history: &ScopeId,
        to: u64,
        origin: &Origin,
    ) -> Result<Option<usize>, Refused> {
        let Some((redo, count)) = self.plan_jump(history, to) else {
            // A replaced step: back to where its line left, then along it.
            return Ok(self
                .jump_replaced(history, to, origin, false)?
                .map(|steps| steps.len()));
        };
        {
            let h = &self.histories[history];
            let list = if redo { &h.redo } else { &h.undo };
            for step in list.iter().rev().take(count) {
                self.can_replay(step)?;
            }
        }
        for _ in 0..count {
            self.walk(history, origin, redo)?;
        }
        Ok(Some(count))
    }

    /// Redos (`true`) or undos, and how many, to bring a history to `to`.
    pub fn plan_jump(&self, history: &ScopeId, to: u64) -> Option<(bool, usize)> {
        let h = self.histories.get(history);
        if to == 0 {
            return Some((false, h.map_or(0, |h| h.undo.len())));
        }
        let h = h?;
        if let Some(i) = h.undo.iter().position(|s| s.id == to) {
            return Some((false, h.undo.len() - 1 - i));
        }
        h.redo
            .iter()
            .position(|s| s.id == to)
            .map(|i| (true, h.redo.len() - i))
    }

    fn walk(
        &mut self,
        history: &ScopeId,
        origin: &Origin,
        redo: bool,
    ) -> Result<Option<Step>, Refused> {
        let Some(h) = self.histories.get_mut(history) else {
            return Ok(None);
        };
        let Some(step) = (if redo { h.redo.pop() } else { h.undo.pop() }) else {
            return Ok(None);
        };
        let replayed = self.replay(&step, origin, !redo);
        let h = self.histories.get_mut(history).expect("still there");
        let (from, to) = if redo {
            (&mut h.redo, &mut h.undo)
        } else {
            (&mut h.undo, &mut h.redo)
        };
        if let Err(e) = replayed {
            from.push(step);
            return Err(e);
        }
        to.push(step.clone());
        h.rev += 1;
        // What comes next never merges with a step walked over.
        h.sealed = true;
        Ok(Some(step))
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
        for h in self.histories.values_mut() {
            let n = h.undo.len() + h.redo.len();
            h.undo.retain(|s| !gone(s));
            h.redo.retain(|s| !gone(s));
            if h.undo.len() + h.redo.len() != n {
                h.rev += 1;
            }
            for b in &mut h.replaced {
                b.steps.retain(|s| !gone(s));
            }
            h.replaced.retain(|b| !b.steps.is_empty());
            for snap in &mut h.snapshots {
                snap.values.retain(|p, _| !paths.contains(p));
            }
        }
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
                        self.write(path, v, origin, Some(OwnerId::FRAMEWORK));
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
