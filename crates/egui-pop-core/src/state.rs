//! The state egui-pop keeps in egui's memory, through typed slots.
//!
//! A [`Slot`] names a value of one type kept in a context's temporary
//! memory (`egui::Memory::data`): per context (`Slot::new`), or per
//! instance (`slot.keyed(widget_id)`, one value per key). Every write
//! registers the slot in the context, so the debug bridge's `state`
//! collector lists what egui-pop holds: each slot's name, type, scope,
//! how many entries it has and a one-line summary of its values.
//!
//! ```
//! use egui_pop_core::state::Slot;
//!
//! // Declared once, beside the code that uses it; the name is unique.
//! const CLICKS: Slot<u32> = Slot::new("example.clicks");
//!
//! let ctx = egui::Context::default();
//! CLICKS.update(&ctx, |n| *n += 1);
//! assert_eq!(CLICKS.get(&ctx), Some(1));
//! // Per instance: one value per key.
//! let widget = egui::Id::new("a button");
//! CLICKS.keyed(widget).set(&ctx, 7);
//! assert_eq!(CLICKS.keyed(widget).get(&ctx), Some(7));
//! assert_eq!(CLICKS.get(&ctx), Some(1));
//! ```
//!
//! egui's context is locked while its memory is read or written, and
//! taking the lock again from inside (`ctx.viewport_id()`,
//! `ctx.input(..)`, another slot) deadlocks. A slot takes the lock
//! itself and only hands its value to the closure of [`Slot::update`];
//! build every key and read every other state *before* calling it. A
//! slot touched from inside another slot's `update` panics with a
//! message instead of deadlocking.

use std::any::TypeId;
use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::fmt::{self, Debug, Write as _};
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

use egui::util::id_type_map::RawKey;
use egui::{Context, Id};
use serde::Serialize;

/// Locks a mutex, ignoring poisoning (a panic caught by a fault boundary
/// must not make the framework's own state unusable).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The longest summary of a value, in characters.
pub const SUMMARY_MAX: usize = 160;

/// A value of type `T` kept in a context's temporary memory, by name.
///
/// Per context by default; [`Slot::keyed`] gives one value per key (a
/// widget's id, a viewport). Names are unique across egui-pop and the
/// application, prefixed by the crate (`core.slider.typing`).
pub struct Slot<T> {
    name: &'static str,
    key: Option<Id>,
    summary: fn(&T) -> String,
    _t: PhantomData<fn() -> T>,
}

impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Slot<T> {}

impl<T> Debug for Slot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slot")
            .field("name", &self.name)
            .field("key", &self.key)
            .finish()
    }
}

/// Where a slot's value lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// One value per context.
    Context,
    /// One value per key (a widget, a viewport, a document).
    Instance,
}

thread_local! {
    /// A slot's closure is running: egui's context is locked.
    static LOCKED: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// Runs `f` with the context marked locked by slot `name`, refusing a
/// slot touched from inside another's closure (it would deadlock).
fn guarded<R>(name: &'static str, f: impl FnOnce() -> R) -> R {
    if let Some(outer) = LOCKED.get() {
        panic!(
            "slot `{name}` touched inside slot `{outer}`'s closure: egui's context is locked there; read it before"
        );
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            LOCKED.set(None);
        }
    }
    LOCKED.set(Some(name));
    let _reset = Reset;
    f()
}

impl<T: Clone + Send + Sync + 'static> Slot<T> {
    /// A slot per context, its values summarised by their `Debug` form.
    pub const fn new(name: &'static str) -> Self
    where
        T: Debug,
    {
        Self {
            name,
            key: None,
            summary: debug_summary::<T>,
            _t: PhantomData,
        }
    }

    /// A slot per context whose values have no `Debug` form (handles,
    /// trait objects): summarised by their type.
    pub const fn opaque(name: &'static str) -> Self {
        Self {
            name,
            key: None,
            summary: opaque_summary::<T>,
            _t: PhantomData,
        }
    }

    /// The same slot, one value per `key` (a widget's id, a viewport).
    pub fn keyed(self, key: impl Hash + Debug) -> Self {
        Self {
            key: Some(Id::new(key)),
            ..self
        }
    }

    /// The slot's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Per context or per instance.
    pub fn scope(&self) -> Scope {
        if self.key.is_some() {
            Scope::Instance
        } else {
            Scope::Context
        }
    }

    /// The id the value is kept under in egui's memory.
    pub fn id(&self) -> Id {
        let base = base_id(self.name);
        match self.key {
            Some(k) => base.with(k),
            None => base,
        }
    }

    /// The value, cloned.
    pub fn get(&self, ctx: &Context) -> Option<T> {
        let id = self.id();
        guarded(self.name, || ctx.data(|d| d.get_temp::<T>(id)))
    }

    /// The value, or `T::default()` (nothing is stored).
    pub fn get_or_default(&self, ctx: &Context) -> T
    where
        T: Default,
    {
        self.get(ctx).unwrap_or_default()
    }

    /// Whether a value is stored.
    pub fn exists(&self, ctx: &Context) -> bool {
        let raw = RawKey::new::<T>(self.id());
        guarded(self.name, || ctx.data(|d| d.get_temp_raw(raw).is_some()))
    }

    /// Stores a value.
    pub fn set(&self, ctx: &Context, value: T) {
        let id = self.id();
        self.register(ctx, id);
        guarded(self.name, || ctx.data_mut(|d| d.insert_temp(id, value)));
    }

    /// Removes the value; what it was.
    pub fn remove(&self, ctx: &Context) -> Option<T> {
        let id = self.id();
        let raw = RawKey::new::<T>(id);
        let old = guarded(self.name, || ctx.data_mut(|d| d.remove_temp_raw(raw)));
        self.unregister(ctx, id);
        old.and_then(|b| b.downcast::<T>().ok()).map(|b| *b)
    }

    /// The value, created by `make` when there is none. `make` runs
    /// outside egui's lock: it may read the context.
    pub fn get_or_insert_with(&self, ctx: &Context, make: impl FnOnce() -> T) -> T {
        if let Some(v) = self.get(ctx) {
            return v;
        }
        let v = make();
        self.set(ctx, v.clone());
        v
    }

    /// Changes the value in place (`T::default()` when there is none)
    /// and returns what `f` returns. `f` runs under egui's lock: it must
    /// not touch the context (no `ctx.viewport_id()`, no other slot).
    pub fn update<R>(&self, ctx: &Context, f: impl FnOnce(&mut T) -> R) -> R
    where
        T: Default,
    {
        let id = self.id();
        self.register(ctx, id);
        guarded(self.name, || {
            ctx.data_mut(|d| f(d.get_temp_mut_or_default::<T>(id)))
        })
    }

    fn register(&self, ctx: &Context, id: Id) {
        let reg = registry(ctx);
        let mut r = lock(&reg);
        let e = r
            .slots
            .entry((self.name, TypeId::of::<T>()))
            .or_insert_with(|| Entry::new::<T>(self.summary));
        if self.key.is_some() {
            e.keyed = true;
            e.ids.insert(id);
        } else {
            e.context = true;
        }
    }

    fn unregister(&self, ctx: &Context, id: Id) {
        if self.key.is_none() {
            return;
        }
        let reg = registry(ctx);
        if let Some(e) = lock(&reg).slots.get_mut(&(self.name, TypeId::of::<T>())) {
            e.ids.remove(&id);
        }
    }
}

fn base_id(name: &'static str) -> Id {
    Id::new(("egui_pop::slot", name))
}

/// A value's `Debug` form, on one line, cut at [`SUMMARY_MAX`].
fn debug_summary<T: Debug>(v: &T) -> String {
    let mut out = Bounded(String::new());
    // Stops early (an error) once the summary is full.
    let _ = write!(out, "{v:?}");
    let mut s: String = out.0.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() >= SUMMARY_MAX {
        s = s.chars().take(SUMMARY_MAX - 1).collect();
        s.push('…');
    }
    s
}

fn opaque_summary<T>(_: &T) -> String {
    format!("<{}>", short_type_name::<T>())
}

/// A writer refusing more than a summary's length, so a large value is
/// not formatted whole.
struct Bounded(String);

impl fmt::Write for Bounded {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // Bytes: at least as many as the characters kept.
        let room = (4 * SUMMARY_MAX).saturating_sub(self.0.len());
        if s.len() <= room {
            self.0.push_str(s);
            Ok(())
        } else {
            let mut end = room;
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            self.0.push_str(&s[..end]);
            Err(fmt::Error)
        }
    }
}

/// A type's name without its module paths.
fn short_type_name<T>() -> String {
    let full = std::any::type_name::<T>();
    let mut out = String::with_capacity(full.len());
    let mut word = String::new();
    for c in full.chars() {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            word.push(c);
        } else {
            out.push_str(word.rsplit("::").next().unwrap_or(&word));
            word.clear();
            out.push(c);
        }
    }
    out.push_str(word.rsplit("::").next().unwrap_or(&word));
    out
}

type Present = fn(&Context, Id) -> bool;
type Summarise = Arc<dyn Fn(&Context, Id) -> Option<String> + Send + Sync>;

/// What the registry knows of a slot.
struct Entry {
    type_name: String,
    /// Written without a key.
    context: bool,
    /// Written with a key.
    keyed: bool,
    /// The keyed entries written and not removed.
    ids: HashSet<Id>,
    present: Present,
    summarise: Summarise,
}

impl Entry {
    fn new<T: Clone + Send + Sync + 'static>(summary: fn(&T) -> String) -> Self {
        Self {
            type_name: short_type_name::<T>(),
            context: false,
            keyed: false,
            ids: HashSet::new(),
            present: |ctx, id| {
                let raw = RawKey::new::<T>(id);
                ctx.data(|d| d.get_temp_raw(raw).is_some())
            },
            // Cloned out, then formatted outside egui's lock.
            summarise: Arc::new(move |ctx, id| {
                let v = ctx.data(|d| d.get_temp::<T>(id));
                v.map(|v| summary(&v))
            }),
        }
    }
}

#[derive(Default)]
struct Registry {
    slots: BTreeMap<(&'static str, TypeId), Entry>,
}

fn registry(ctx: &Context) -> Arc<Mutex<Registry>> {
    let id = Id::new("egui_pop::slot::registry");
    guarded("registry", || {
        ctx.data_mut(|d| {
            d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(Registry::default())))
                .clone()
        })
    })
}

/// A slot as the debug bridge reports it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SlotReport {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub scope: Scope,
    /// Values held: 0 or 1 per context, one per key per instance.
    pub entries: usize,
    /// The first values' summaries (the value's, per context).
    pub summaries: Vec<String>,
}

/// Every slot written in a context, by name, with its entries still
/// held (keys removed since are forgotten). At most `samples`
/// summaries per slot.
pub fn report(ctx: &Context, samples: usize) -> Vec<SlotReport> {
    // Copied out of the registry: reading values takes egui's lock, and
    // a summary may lock what it summarises.
    struct Seen {
        name: &'static str,
        type_id: TypeId,
        type_name: String,
        context: bool,
        keyed: bool,
        ids: Vec<Id>,
        present: Present,
        summarise: Summarise,
    }
    let reg = registry(ctx);
    let seen: Vec<Seen> = lock(&reg)
        .slots
        .iter()
        .map(|((name, type_id), e)| Seen {
            name,
            type_id: *type_id,
            type_name: e.type_name.clone(),
            context: e.context,
            keyed: e.keyed,
            ids: e.ids.iter().copied().collect(),
            present: e.present,
            summarise: e.summarise.clone(),
        })
        .collect();
    let mut out = Vec::new();
    for s in seen {
        if s.context {
            let base = base_id(s.name);
            let held = (s.present)(ctx, base);
            let summaries = if held && samples > 0 {
                (s.summarise)(ctx, base).into_iter().collect()
            } else {
                Vec::new()
            };
            out.push(SlotReport {
                name: s.name.into(),
                type_name: s.type_name.clone(),
                scope: Scope::Context,
                entries: usize::from(held),
                summaries,
            });
        }
        if s.keyed {
            let (mut held, gone): (Vec<Id>, Vec<Id>) =
                s.ids.into_iter().partition(|id| (s.present)(ctx, *id));
            if !gone.is_empty()
                && let Some(e) = lock(&reg).slots.get_mut(&(s.name, s.type_id))
            {
                for id in &gone {
                    e.ids.remove(id);
                }
            }
            held.sort_by_key(Id::value);
            let summaries = held
                .iter()
                .take(samples)
                .filter_map(|id| (s.summarise)(ctx, *id))
                .collect();
            out.push(SlotReport {
                name: s.name.into(),
                type_name: s.type_name,
                scope: Scope::Instance,
                entries: held.len(),
                summaries,
            });
        }
    }
    out
}

/// Shared state of type `T` in a context, created on first use.
pub(crate) fn shared<T: Default + Debug + Send + 'static>(
    ctx: &Context,
    name: &'static str,
) -> Arc<Mutex<T>> {
    Slot::<Shared<T>>::new(name)
        .get_or_insert_with(ctx, || Shared(Arc::new(Mutex::new(T::default()))))
        .0
}

/// A shared value, summarised by what it holds.
struct Shared<T>(Arc<Mutex<T>>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Debug> Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.try_lock() {
            Ok(v) => v.fmt(f),
            Err(TryLockError::Poisoned(p)) => p.into_inner().fmt(f),
            Err(TryLockError::WouldBlock) => f.write_str("<in use>"),
        }
    }
}

/// The pass being drawn.
pub(crate) fn pass(ctx: &Context) -> u64 {
    ctx.cumulative_pass_nr()
}

#[cfg(test)]
mod tests {
    use super::*;

    const COUNT: Slot<u32> = Slot::new("test.count");
    const TEXT: Slot<String> = Slot::new("test.text");

    #[test]
    fn per_context_and_per_instance_values_are_apart() {
        let ctx = Context::default();
        assert_eq!(COUNT.get(&ctx), None);
        COUNT.set(&ctx, 3);
        COUNT.keyed(1).set(&ctx, 10);
        COUNT.keyed(2).set(&ctx, 20);
        assert_eq!(COUNT.get(&ctx), Some(3));
        assert_eq!(COUNT.keyed(1).get(&ctx), Some(10));
        assert_eq!(COUNT.update(&ctx, |n| std::mem::replace(n, 4)), 3);
        assert_eq!(COUNT.remove(&ctx), Some(4));
        assert!(!COUNT.exists(&ctx));
        assert_eq!(COUNT.keyed(2).remove(&ctx), Some(20));
    }

    #[test]
    fn the_report_counts_held_entries_and_summarises_them() {
        let ctx = Context::default();
        TEXT.set(&ctx, "x".repeat(1000));
        for k in 0..5 {
            COUNT.keyed(k).set(&ctx, k);
        }
        COUNT.keyed(0).remove(&ctx);
        // Removed behind the slot's back: forgotten by the next report.
        let gone = COUNT.keyed(1).id();
        ctx.data_mut(|d| d.remove::<u32>(gone));
        let r = report(&ctx, 2);
        let count = r.iter().find(|s| s.name == "test.count").unwrap();
        assert_eq!((count.scope, count.entries), (Scope::Instance, 3));
        assert_eq!(count.summaries.len(), 2);
        assert_eq!(count.type_name, "u32");
        let text = r.iter().find(|s| s.name == "test.text").unwrap();
        assert_eq!((text.scope, text.entries), (Scope::Context, 1));
        assert_eq!(text.summaries[0].chars().count(), SUMMARY_MAX);
        assert!(text.summaries[0].ends_with('…'));
    }

    #[test]
    fn opaque_values_are_summarised_by_their_type() {
        let ctx = Context::default();
        let s: Slot<Arc<dyn Fn() + Send + Sync>> = Slot::opaque("test.opaque");
        s.set(&ctx, Arc::new(|| {}));
        let r = report(&ctx, 1);
        let o = r.iter().find(|s| s.name == "test.opaque").unwrap();
        assert_eq!(o.summaries, ["<Arc<dyn Fn() + Send + Sync>>"]);
    }

    #[test]
    fn the_debug_bridge_lists_the_slots() {
        let ctx = Context::default();
        COUNT.keyed("a").set(&ctx, 1);
        let mut bridge = crate::debug::DebugBridge::with_core_collectors();
        let snap = bridge.snapshot_of(&ctx, "state").unwrap();
        let slot = snap["slots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "test.count")
            .unwrap();
        assert_eq!(slot["scope"], "instance");
        assert_eq!(slot["entries"], 1);
        let f = bridge.call(
            &ctx,
            "state",
            "slot",
            &serde_json::json!({ "name": "test.count" }),
        );
        assert!(f.ok && !f.mutates, "{}", f.message);
        assert_eq!(f.data[0]["summaries"][0], "1");
        let f = bridge.call(
            &ctx,
            "state",
            "slot",
            &serde_json::json!({ "name": "nothing" }),
        );
        assert!(!f.ok);
    }

    #[test]
    #[should_panic(expected = "touched inside slot `test.count`")]
    fn a_slot_inside_another_s_closure_panics_instead_of_deadlocking() {
        let ctx = Context::default();
        COUNT.update(&ctx, |_| TEXT.get(&ctx));
    }
}
