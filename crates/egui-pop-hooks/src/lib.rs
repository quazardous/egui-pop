#![doc = include_str!("../README.md")]
#![doc = "\n\n<div class=\"warning\">Experimental: may change in any release\n(docs/stability.md).</div>"]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod order;

use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

/// The bus contract this kernel implements; its types are re-exported
/// below under their old paths.
pub use bus_contract;
pub use bus_contract::{
    Altered, Collected, DEFAULT_DEADLINE, FAILED_FACT, Failure, FailureKind, HookError, LAST,
    ListenOpts, ListenerInfo, PLUGIN_DEADLINE, Point, PointDecl, PointInfo, PointKind, PointSpec,
    Refused, declare_all, missing_targets, order_dump,
};
use bus_contract::{AnyInput, AnyListener, AnyValue, Bus, CloneFn, FailureSink, FreshFn, PointKey};

/// A point's listeners in order, the point's deadline, and the failures
/// known before running.
type Snapshot = (Vec<Arc<Entry>>, Duration, Vec<Failure>);

struct Entry {
    id: String,
    opts: ListenOpts,
    seq: u64,
    run: AnyListener,
    runs: AtomicU64,
    failures: AtomicU64,
    last_failure: Mutex<Option<String>>,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("id", &self.id)
            .field("opts", &self.opts)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Slot {
    type_id: TypeId,
    kind: PointKind,
    spec: Option<PointSpec>,
    /// In the order they run.
    ordered: Vec<Arc<Entry>>,
    /// `(listener, target)` constraints naming a missing listener.
    missing: Vec<(String, String)>,
}

impl Slot {
    fn new(key: &PointKey) -> Slot {
        Slot {
            type_id: key.type_id,
            kind: key.kind,
            spec: None,
            ordered: Vec::new(),
            missing: Vec::new(),
        }
    }

    /// Orders `entries` into the slot, or returns the cycle.
    fn set(&mut self, entries: Vec<Arc<Entry>>) -> Result<(), Vec<String>> {
        let nodes: Vec<order::Node<'_>> = entries
            .iter()
            .map(|e| order::Node {
                id: &e.id,
                after: &e.opts.after,
                before: &e.opts.before,
                priority: e.opts.priority,
                seq: e.seq,
            })
            .collect();
        let resolved = order::resolve(&nodes)?;
        self.ordered = resolved.order.iter().map(|&i| entries[i].clone()).collect();
        self.missing = resolved.missing;
        Ok(())
    }
}

/// The hook kernel: hook points, their listeners, emits and
/// introspection. `Send + Sync`: share it in an `Arc`. Listeners run with
/// no lock held, so a listener may emit or listen itself.
pub struct Kernel {
    points: RwLock<BTreeMap<&'static str, Slot>>,
    sinks: RwLock<Vec<FailureSink>>,
    default_deadline: Duration,
    seq: AtomicU64,
}

impl fmt::Debug for Kernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let points = self.read();
        f.debug_struct("Kernel")
            .field("points", &points.keys().collect::<Vec<_>>())
            .field("default_deadline", &self.default_deadline)
            .finish_non_exhaustive()
    }
}

impl Default for Kernel {
    fn default() -> Kernel {
        Kernel::new()
    }
}

impl Kernel {
    /// An empty kernel whose listeners get [`DEFAULT_DEADLINE`] unless
    /// they or their point set another.
    pub fn new() -> Kernel {
        Kernel::with_default_deadline(DEFAULT_DEADLINE)
    }

    /// An empty kernel with another default deadline.
    pub fn with_default_deadline(deadline: Duration) -> Kernel {
        Kernel {
            points: RwLock::new(BTreeMap::new()),
            sinks: RwLock::new(Vec::new()),
            default_deadline: deadline,
            seq: AtomicU64::new(0),
        }
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, BTreeMap<&'static str, Slot>> {
        self.points.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<&'static str, Slot>> {
        self.points.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Declares the point `P`: its description and deadline. Listeners may
    /// register before or after; declaring twice is refused.
    pub fn declare<P: Point>(&self, spec: PointSpec) -> Result<(), Refused> {
        self.declare_point(PointKey::of::<P>(), spec)
    }

    /// Declares a point by its key ([`Bus::declare_point`]).
    pub fn declare_point(&self, key: PointKey, spec: PointSpec) -> Result<(), Refused> {
        let mut points = self.write();
        let slot = points.entry(key.id).or_insert_with(|| Slot::new(&key));
        if slot.type_id != key.type_id {
            return Err(Refused::TypeMismatch { point: key.id });
        }
        if slot.spec.is_some() {
            return Err(Refused::AlreadyDeclared { point: key.id });
        }
        slot.spec = Some(spec);
        Ok(())
    }

    /// Registers a listener on `P` under `id`. It reads the emit's input
    /// and edits the value; its edit counts only when it returns `Ok`
    /// within its deadline without panicking.
    ///
    /// Refused, with nothing changed, when the id is taken on the point
    /// or when its `after`/`before` close a cycle. A constraint naming a
    /// listener not (yet) registered is accepted, reported by each emit
    /// and by introspection, and ignored.
    pub fn listen<P: Point>(
        &self,
        id: impl Into<String>,
        opts: ListenOpts,
        run: impl Fn(&P::Input, &mut P::Value) -> Result<(), HookError> + Send + Sync + 'static,
    ) -> Result<(), Refused> {
        self.listen_any(
            PointKey::of::<P>(),
            id.into(),
            opts,
            bus_contract::erase::<P>(run),
        )
    }

    /// Registers an erased listener by its point's key
    /// ([`Bus::listen_any`]).
    pub fn listen_any(
        &self,
        key: PointKey,
        id: String,
        opts: ListenOpts,
        run: AnyListener,
    ) -> Result<(), Refused> {
        let mut points = self.write();
        let slot = points.entry(key.id).or_insert_with(|| Slot::new(&key));
        if slot.type_id != key.type_id {
            return Err(Refused::TypeMismatch { point: key.id });
        }
        if slot.ordered.iter().any(|e| e.id == id) {
            return Err(Refused::Duplicate {
                point: key.id,
                listener: id,
            });
        }
        let mut entries = slot.ordered.clone();
        entries.push(Arc::new(Entry {
            id,
            opts,
            seq: self.seq.fetch_add(1, Ordering::Relaxed),
            run,
            runs: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            last_failure: Mutex::new(None),
        }));
        slot.set(entries).map_err(|path| Refused::Cycle {
            point: key.id,
            path,
        })
    }

    /// Removes the listener `id` of the point `point`; whether it was
    /// there.
    pub fn unlisten(&self, point: &str, id: &str) -> bool {
        let mut points = self.write();
        let Some(slot) = points.get_mut(point) else {
            return false;
        };
        let before = slot.ordered.len();
        let entries: Vec<_> = slot
            .ordered
            .iter()
            .filter(|e| e.id != id)
            .cloned()
            .collect();
        let removed = entries.len() != before;
        // Removing a listener cannot close a cycle.
        let _ = slot.set(entries);
        removed
    }

    /// Removes every listener of `owner`, on every point (a plugin
    /// uninstalled, a module switched off); how many were removed.
    pub fn forget(&self, owner: &str) -> usize {
        let mut points = self.write();
        let mut removed = 0;
        for slot in points.values_mut() {
            let before = slot.ordered.len();
            let entries: Vec<_> = slot
                .ordered
                .iter()
                .filter(|e| e.opts.owner.as_deref() != Some(owner))
                .cloned()
                .collect();
            removed += before - entries.len();
            let _ = slot.set(entries);
        }
        removed
    }

    /// Adds a sink called with each failure that dropped a contribution
    /// ([`Failure::dropped`]), as it happens: where the host publishes
    /// its [`FAILED_FACT`] fact or logs. A sink that panics is ignored.
    pub fn on_failure(&self, sink: impl Fn(&Failure) + Send + Sync + 'static) {
        self.add_failure_sink(Arc::new(sink));
    }

    fn add_failure_sink(&self, sink: FailureSink) {
        self.sinks
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(sink);
    }

    /// Passes `value` through `P`'s listeners in order: each edits the
    /// value the previous ones left. A listener that fails leaves it as
    /// it found it.
    pub fn alter<P: Point>(&self, input: P::Input, value: P::Value) -> Altered<P::Value>
    where
        P::Value: Clone,
    {
        (self as &dyn Bus).alter::<P>(input, value)
    }

    /// Gives each of `P`'s listeners, in order, a fresh default value to
    /// fill, and gathers the values of those that succeed.
    pub fn collect<P: Point>(&self, input: P::Input) -> Collected<P::Value>
    where
        P::Value: Default,
    {
        (self as &dyn Bus).collect::<P>(input)
    }

    fn alter_erased(
        &self,
        key: PointKey,
        input: AnyInput,
        value: AnyValue,
        clone: CloneFn,
    ) -> Altered<AnyValue> {
        let (entries, deadline, mut failures) = self.snapshot(&key);
        let mut value = value;
        for entry in &entries {
            match call(
                key.id,
                entry,
                &input,
                clone(&*value),
                entry.opts.deadline.unwrap_or(deadline),
            ) {
                Ok(v) => value = v,
                Err(kind) => failures.push(self.failed(key.id, entry, kind)),
            }
        }
        Altered { value, failures }
    }

    fn collect_erased(
        &self,
        key: PointKey,
        input: AnyInput,
        fresh: FreshFn,
    ) -> Collected<AnyValue> {
        let (entries, deadline, mut failures) = self.snapshot(&key);
        let mut parts = Vec::with_capacity(entries.len());
        for entry in &entries {
            match call(
                key.id,
                entry,
                &input,
                fresh(),
                entry.opts.deadline.unwrap_or(deadline),
            ) {
                Ok(v) => parts.push((entry.id.clone(), v)),
                Err(kind) => failures.push(self.failed(key.id, entry, kind)),
            }
        }
        Collected { parts, failures }
    }

    /// The listeners of a point in order, the point's deadline, and the
    /// failures known before running (missing targets, a type mismatch).
    /// Taken under the lock, used without it.
    fn snapshot(&self, key: &PointKey) -> Snapshot {
        let points = self.read();
        let default = self.default_deadline;
        let Some(slot) = points.get(key.id) else {
            return (Vec::new(), default, Vec::new());
        };
        if slot.type_id != key.type_id {
            let failure = Failure {
                point: key.id,
                listener: "*".to_owned(),
                owner: None,
                kind: FailureKind::TypeMismatch,
            };
            return (Vec::new(), default, vec![failure]);
        }
        let point_deadline = slot
            .spec
            .as_ref()
            .and_then(|s| s.deadline)
            .unwrap_or(default);
        let failures = slot
            .missing
            .iter()
            .map(|(listener, target)| Failure {
                point: key.id,
                listener: listener.clone(),
                owner: slot
                    .ordered
                    .iter()
                    .find(|e| &e.id == listener)
                    .and_then(|e| e.opts.owner.clone()),
                kind: FailureKind::MissingTarget(target.clone()),
            })
            .collect();
        (slot.ordered.clone(), point_deadline, failures)
    }

    fn failed(&self, point: &'static str, entry: &Entry, kind: FailureKind) -> Failure {
        let failure = Failure {
            point,
            listener: entry.id.clone(),
            owner: entry.opts.owner.clone(),
            kind,
        };
        entry.failures.fetch_add(1, Ordering::Relaxed);
        *entry
            .last_failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(failure.to_string());
        let sinks: Vec<FailureSink> = self
            .sinks
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for sink in sinks {
            let _ = catch_unwind(AssertUnwindSafe(|| sink(&failure)));
        }
        failure
    }

    /// Every point, in id order, with its listeners in the order they
    /// run.
    pub fn points(&self) -> Vec<PointInfo> {
        self.read()
            .iter()
            .map(|(id, slot)| self.point_info(id, slot))
            .collect()
    }

    /// The listeners of the point `point`, in the order they run; `None`
    /// for a point nobody declared or listened to.
    pub fn listeners(&self, point: &str) -> Option<Vec<ListenerInfo>> {
        self.read()
            .get_key_value(point)
            .map(|(id, slot)| self.point_info(id, slot).listeners)
    }

    fn point_info(&self, id: &'static str, slot: &Slot) -> PointInfo {
        let deadline = slot
            .spec
            .as_ref()
            .and_then(|s| s.deadline)
            .unwrap_or(self.default_deadline);
        PointInfo {
            id,
            kind: slot.kind,
            description: slot
                .spec
                .as_ref()
                .map(|s| s.description.clone())
                .unwrap_or_default(),
            declared: slot.spec.is_some(),
            deadline,
            listeners: slot
                .ordered
                .iter()
                .map(|e| ListenerInfo {
                    id: e.id.clone(),
                    owner: e.opts.owner.clone(),
                    after: e.opts.after.clone(),
                    before: e.opts.before.clone(),
                    priority: e.opts.priority,
                    deadline: e.opts.deadline.unwrap_or(deadline),
                    threaded: e.opts.threaded,
                    missing: slot
                        .missing
                        .iter()
                        .filter(|(l, _)| *l == e.id)
                        .map(|(_, t)| t.clone())
                        .collect(),
                    runs: e.runs.load(Ordering::Relaxed),
                    failures: e.failures.load(Ordering::Relaxed),
                    last_failure: e
                        .last_failure
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone(),
                })
                .collect(),
        }
    }
}

/// Runs one listener on `value`, inline or on its own thread, within its
/// deadline, panics caught.
fn call(
    point: &'static str,
    entry: &Entry,
    input: &AnyInput,
    value: AnyValue,
    deadline: Duration,
) -> Result<AnyValue, FailureKind> {
    entry.runs.fetch_add(1, Ordering::Relaxed);
    if !entry.opts.threaded {
        let start = Instant::now();
        let result = guarded(&entry.run, &**input, value);
        let took = start.elapsed();
        return match result {
            Ok(_) if took > deadline => Err(FailureKind::Late {
                deadline,
                took: Some(took),
            }),
            r => r,
        };
    }
    let (tx, rx) = sync_channel(1);
    let run = entry.run.clone();
    let input = input.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("hook {point} {}", entry.id))
        .spawn(move || {
            // The emit may have stopped waiting: then nobody receives.
            let _ = tx.send(guarded(&run, &*input, value));
        });
    if let Err(e) = spawned {
        return Err(FailureKind::Error(format!(
            "could not start its thread: {e}"
        )));
    }
    match rx.recv_timeout(deadline) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(FailureKind::Late {
            deadline,
            took: None,
        }),
        Err(RecvTimeoutError::Disconnected) => Err(FailureKind::Panic(
            "its thread ended without an answer".to_owned(),
        )),
    }
}

fn guarded(
    run: &AnyListener,
    input: &(dyn Any + Send + Sync),
    mut value: AnyValue,
) -> Result<AnyValue, FailureKind> {
    match catch_unwind(AssertUnwindSafe(|| run(input, &mut *value))) {
        Ok(Ok(())) => Ok(value),
        Ok(Err(e)) => Err(FailureKind::Error(e.0)),
        Err(payload) => Err(FailureKind::Panic(panic_message(payload.as_ref()))),
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic without a message".to_owned()
    }
}

impl Bus for Kernel {
    fn declare_point(&self, point: PointKey, spec: PointSpec) -> Result<(), Refused> {
        Kernel::declare_point(self, point, spec)
    }

    fn listen_any(
        &self,
        point: PointKey,
        id: String,
        opts: ListenOpts,
        run: AnyListener,
    ) -> Result<(), Refused> {
        Kernel::listen_any(self, point, id, opts, run)
    }

    fn unlisten(&self, point: &str, id: &str) -> bool {
        Kernel::unlisten(self, point, id)
    }

    fn forget(&self, owner: &str) -> usize {
        Kernel::forget(self, owner)
    }

    fn alter_any(
        &self,
        point: PointKey,
        input: AnyInput,
        value: AnyValue,
        clone: CloneFn,
    ) -> Altered<AnyValue> {
        self.alter_erased(point, input, value, clone)
    }

    fn collect_any(&self, point: PointKey, input: AnyInput, fresh: FreshFn) -> Collected<AnyValue> {
        self.collect_erased(point, input, fresh)
    }

    fn add_failure_sink(&self, sink: FailureSink) {
        Kernel::add_failure_sink(self, sink)
    }

    fn points(&self) -> Vec<PointInfo> {
        Kernel::points(self)
    }

    fn listeners(&self, point: &str) -> Option<Vec<ListenerInfo>> {
        Kernel::listeners(self, point)
    }
}
