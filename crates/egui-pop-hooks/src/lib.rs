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

/// The deadline of an in-process listener when neither the listener nor
/// its point sets one.
pub const DEFAULT_DEADLINE: Duration = Duration::from_secs(1);

/// The deadline meant for listeners that answer from another process (a
/// plugin over RPC): give it with [`ListenOpts::deadline`] or
/// [`PointSpec::deadline`].
pub const PLUGIN_DEADLINE: Duration = Duration::from_secs(5);

/// The name a host gives the fact it publishes for each [`Failure`] its
/// sinks receive ([`Kernel::on_failure`]).
pub const FAILED_FACT: &str = "hook.failed";

/// A hook point: one type per point, so its input and value are checked
/// at compile time. Declared by the crate that owns the result.
///
/// Every listener of a point has the same shape: it reads the emit's
/// [`Input`](Point::Input) and edits a [`Value`](Point::Value). An
/// [`alter`](Kernel::alter) emit passes one value through the listeners
/// in order; a [`collect`](Kernel::collect) emit gives each listener a
/// fresh value and gathers them in a list.
pub trait Point: 'static {
    /// The point's id, dotted by convention (`health.report`).
    const ID: &'static str;
    /// What every listener reads: shared, never changed by a listener.
    type Input: Send + Sync + 'static;
    /// What a listener edits: the value altered in turn, or one
    /// listener's contribution to a collect.
    type Value: Send + 'static;
}

/// A point's declaration: its description and limits, for introspection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointSpec {
    /// What the point builds, for people and agents reading
    /// [`Kernel::points`].
    pub description: String,
    /// The deadline of the point's listeners that set none themselves;
    /// `None` takes the kernel's default.
    pub deadline: Option<Duration>,
}

impl PointSpec {
    /// A point described by `description`, with the kernel's deadline.
    pub fn new(description: impl Into<String>) -> PointSpec {
        PointSpec {
            description: description.into(),
            deadline: None,
        }
    }

    /// The deadline of the point's listeners that set none themselves.
    pub fn deadline(mut self, deadline: Duration) -> PointSpec {
        self.deadline = Some(deadline);
        self
    }
}

/// How a listener is placed and limited.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListenOpts {
    /// Listeners this one runs after.
    pub after: Vec<String>,
    /// Listeners this one runs before.
    pub before: Vec<String>,
    /// Among listeners `after`/`before` leave free, higher runs first;
    /// ties run in registration order.
    pub priority: i32,
    /// Overrides the point's deadline.
    pub deadline: Option<Duration>,
    /// Who registered it (a module, a plugin), for [`Kernel::forget`] and
    /// introspection.
    pub owner: Option<String>,
    /// Runs on a thread of its own, so the emit stops waiting at the
    /// deadline. An inline listener (the default) runs on the emitting
    /// thread: it cannot be stopped, and only counts as late once it
    /// returns.
    pub threaded: bool,
}

impl ListenOpts {
    /// No constraint, priority 0, the point's deadline, inline.
    pub fn new() -> ListenOpts {
        ListenOpts::default()
    }

    /// Runs after the listener `id` (a missing one is reported and
    /// ignored).
    pub fn after(mut self, id: impl Into<String>) -> ListenOpts {
        self.after.push(id.into());
        self
    }

    /// Runs before the listener `id` (a missing one is reported and
    /// ignored).
    pub fn before(mut self, id: impl Into<String>) -> ListenOpts {
        self.before.push(id.into());
        self
    }

    /// Higher runs first among listeners left free by `after`/`before`.
    pub fn priority(mut self, priority: i32) -> ListenOpts {
        self.priority = priority;
        self
    }

    /// Overrides the point's deadline.
    pub fn deadline(mut self, deadline: Duration) -> ListenOpts {
        self.deadline = Some(deadline);
        self
    }

    /// Who registered the listener.
    pub fn owner(mut self, owner: impl Into<String>) -> ListenOpts {
        self.owner = Some(owner.into());
        self
    }

    /// Runs on a thread of its own, so a slow listener cannot hold the
    /// emit past its deadline.
    pub fn threaded(mut self) -> ListenOpts {
        self.threaded = true;
        self
    }
}

/// The error a listener returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookError(pub String);

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HookError {}

impl From<String> for HookError {
    fn from(message: String) -> HookError {
        HookError(message)
    }
}

impl From<&str> for HookError {
    fn from(message: &str) -> HookError {
        HookError(message.to_owned())
    }
}

/// What went wrong with one listener during an emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The listener returned an error.
    Error(String),
    /// The listener panicked; the panic's message.
    Panic(String),
    /// The listener missed its deadline. `took` is its time when it
    /// returned late (inline); `None` when the emit stopped waiting
    /// (threaded).
    Late {
        /// The deadline it had.
        deadline: Duration,
        /// How long it ran, when it returned.
        took: Option<Duration>,
    },
    /// An `after` or `before` constraint names a listener that is not
    /// registered. The listener still ran and its contribution counts;
    /// only the constraint was ignored. Not sent to the failure sinks.
    MissingTarget(String),
    /// The point id is registered with another [`Point`] type; no
    /// listener ran. `listener` is `*`.
    TypeMismatch,
}

/// One listener's failure: its contribution was dropped (except for a
/// [`FailureKind::MissingTarget`]), the other listeners ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The point emitted.
    pub point: &'static str,
    /// The listener's id.
    pub listener: String,
    /// The listener's owner.
    pub owner: Option<String>,
    /// What went wrong.
    pub kind: FailureKind,
}

impl Failure {
    /// Whether the failure dropped a contribution: everything but a
    /// missing ordering target. Only these reach the failure sinks.
    pub fn dropped(&self) -> bool {
        !matches!(self.kind, FailureKind::MissingTarget(_))
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "hook `{}`, listener `{}`: ", self.point, self.listener)?;
        match &self.kind {
            FailureKind::Error(m) => write!(f, "failed: {m}"),
            FailureKind::Panic(m) => write!(f, "panicked: {m}"),
            FailureKind::Late {
                deadline,
                took: Some(t),
            } => write!(f, "took {t:?}, past its deadline of {deadline:?}"),
            FailureKind::Late { deadline, .. } => {
                write!(f, "no answer within its deadline of {deadline:?}")
            }
            FailureKind::MissingTarget(t) => {
                write!(f, "orders itself against `{t}`, which is not there")
            }
            FailureKind::TypeMismatch => write!(f, "the point is registered with another type"),
        }
    }
}

/// Why a declaration or a listener was refused. Nothing was changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The point already has a listener with this id.
    Duplicate {
        /// The point.
        point: &'static str,
        /// The id taken.
        listener: String,
    },
    /// The listener's `after`/`before` would close a cycle: these
    /// listeners, each to run before the next, the first again at the
    /// end.
    Cycle {
        /// The point.
        point: &'static str,
        /// The listeners in the cycle.
        path: Vec<String>,
    },
    /// The point id is already registered with another [`Point`] type.
    TypeMismatch {
        /// The point.
        point: &'static str,
    },
    /// The point was already declared.
    AlreadyDeclared {
        /// The point.
        point: &'static str,
    },
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::Duplicate { point, listener } => {
                write!(f, "hook `{point}` already has a listener `{listener}`")
            }
            Refused::Cycle { point, path } => {
                write!(
                    f,
                    "hook `{point}`: listeners order each other in a cycle: {}",
                    path.join(" -> ")
                )
            }
            Refused::TypeMismatch { point } => {
                write!(f, "hook `{point}` is registered with another type")
            }
            Refused::AlreadyDeclared { point } => write!(f, "hook `{point}` is already declared"),
        }
    }
}

impl std::error::Error for Refused {}

/// The result of an [`alter`](Kernel::alter) emit.
#[derive(Debug)]
pub struct Altered<V> {
    /// The value after every listener that succeeded.
    pub value: V,
    /// Every failure, in listener order.
    pub failures: Vec<Failure>,
}

/// The result of a [`collect`](Kernel::collect) emit.
#[derive(Debug)]
pub struct Collected<V> {
    /// Each successful listener's value, in listener order, with its id.
    pub parts: Vec<(String, V)>,
    /// Every failure, in listener order.
    pub failures: Vec<Failure>,
}

/// A listener as introspection shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListenerInfo {
    /// Its id.
    pub id: String,
    /// Its owner.
    pub owner: Option<String>,
    /// The listeners it runs after.
    pub after: Vec<String>,
    /// The listeners it runs before.
    pub before: Vec<String>,
    /// Its priority.
    pub priority: i32,
    /// The deadline it gets: its own, else its point's, else the kernel's.
    pub deadline: Duration,
    /// Whether it runs on a thread of its own.
    pub threaded: bool,
    /// The `after`/`before` targets that are not registered.
    pub missing: Vec<String>,
    /// How many times it ran.
    pub runs: u64,
    /// How many of those runs failed (contribution dropped).
    pub failures: u64,
    /// The last failure, as text.
    pub last_failure: Option<String>,
}

/// A point as introspection shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointInfo {
    /// Its id.
    pub id: &'static str,
    /// Its description; empty until declared.
    pub description: String,
    /// Whether its owner declared it ([`Kernel::declare`]); a point
    /// listened to before that is not.
    pub declared: bool,
    /// The deadline of its listeners that set none.
    pub deadline: Duration,
    /// Its listeners, in the order they run.
    pub listeners: Vec<ListenerInfo>,
}

type ListenerFn<P> = Arc<
    dyn Fn(&<P as Point>::Input, &mut <P as Point>::Value) -> Result<(), HookError> + Send + Sync,
>;

/// A point's listeners in order with their functions, the point's
/// deadline, and the failures known before running.
type Snapshot<P> = (Vec<(Arc<Entry>, ListenerFn<P>)>, Duration, Vec<Failure>);

type Sink = Arc<dyn Fn(&Failure) + Send + Sync>;

struct Entry {
    id: String,
    opts: ListenOpts,
    seq: u64,
    /// A `ListenerFn<P>` of the point's type.
    run: Box<dyn Any + Send + Sync>,
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
    spec: Option<PointSpec>,
    /// In the order they run.
    ordered: Vec<Arc<Entry>>,
    /// `(listener, target)` constraints naming a missing listener.
    missing: Vec<(String, String)>,
}

impl Slot {
    fn new(type_id: TypeId) -> Slot {
        Slot {
            type_id,
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
    sinks: RwLock<Vec<Sink>>,
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

    /// Declares the point `P`: its description and deadline. Listeners
    /// may register before or after; declaring twice is refused.
    pub fn declare<P: Point>(&self, spec: PointSpec) -> Result<(), Refused> {
        let mut points = self.write();
        let slot = points
            .entry(P::ID)
            .or_insert_with(|| Slot::new(TypeId::of::<P>()));
        if slot.type_id != TypeId::of::<P>() {
            return Err(Refused::TypeMismatch { point: P::ID });
        }
        if slot.spec.is_some() {
            return Err(Refused::AlreadyDeclared { point: P::ID });
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
        let id = id.into();
        let run: ListenerFn<P> = Arc::new(run);
        let mut points = self.write();
        let slot = points
            .entry(P::ID)
            .or_insert_with(|| Slot::new(TypeId::of::<P>()));
        if slot.type_id != TypeId::of::<P>() {
            return Err(Refused::TypeMismatch { point: P::ID });
        }
        if slot.ordered.iter().any(|e| e.id == id) {
            return Err(Refused::Duplicate {
                point: P::ID,
                listener: id,
            });
        }
        let mut entries = slot.ordered.clone();
        entries.push(Arc::new(Entry {
            id,
            opts,
            seq: self.seq.fetch_add(1, Ordering::Relaxed),
            run: Box::new(run),
            runs: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            last_failure: Mutex::new(None),
        }));
        slot.set(entries)
            .map_err(|path| Refused::Cycle { point: P::ID, path })
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
        self.sinks
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Arc::new(sink));
    }

    /// Passes `value` through `P`'s listeners in order: each edits the
    /// value the previous ones left. A listener that fails leaves it as
    /// it found it.
    pub fn alter<P: Point>(&self, input: P::Input, value: P::Value) -> Altered<P::Value>
    where
        P::Value: Clone,
    {
        let (entries, deadline, mut failures) = self.snapshot::<P>();
        let input = Arc::new(input);
        let mut value = value;
        for (entry, run) in &entries {
            match call::<P>(
                entry,
                run,
                &input,
                value.clone(),
                entry.opts.deadline.unwrap_or(deadline),
            ) {
                Ok(v) => value = v,
                Err(kind) => failures.push(self.failed::<P>(entry, kind)),
            }
        }
        Altered { value, failures }
    }

    /// Gives each of `P`'s listeners, in order, a fresh default value to
    /// fill, and gathers the values of those that succeed.
    pub fn collect<P: Point>(&self, input: P::Input) -> Collected<P::Value>
    where
        P::Value: Default,
    {
        let (entries, deadline, mut failures) = self.snapshot::<P>();
        let input = Arc::new(input);
        let mut parts = Vec::with_capacity(entries.len());
        for (entry, run) in &entries {
            match call::<P>(
                entry,
                run,
                &input,
                P::Value::default(),
                entry.opts.deadline.unwrap_or(deadline),
            ) {
                Ok(v) => parts.push((entry.id.clone(), v)),
                Err(kind) => failures.push(self.failed::<P>(entry, kind)),
            }
        }
        Collected { parts, failures }
    }

    /// The listeners of `P` in order with their functions, the point's
    /// deadline, and the failures known before running (missing targets,
    /// a type mismatch). Taken under the lock, used without it.
    fn snapshot<P: Point>(&self) -> Snapshot<P> {
        let points = self.read();
        let default = self.default_deadline;
        let Some(slot) = points.get(P::ID) else {
            return (Vec::new(), default, Vec::new());
        };
        if slot.type_id != TypeId::of::<P>() {
            let failure = Failure {
                point: P::ID,
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
        let entries = slot
            .ordered
            .iter()
            .filter_map(|e| {
                let run = e.run.downcast_ref::<ListenerFn<P>>()?.clone();
                Some((e.clone(), run))
            })
            .collect();
        let failures = slot
            .missing
            .iter()
            .map(|(listener, target)| Failure {
                point: P::ID,
                listener: listener.clone(),
                owner: slot
                    .ordered
                    .iter()
                    .find(|e| &e.id == listener)
                    .and_then(|e| e.opts.owner.clone()),
                kind: FailureKind::MissingTarget(target.clone()),
            })
            .collect();
        (entries, point_deadline, failures)
    }

    fn failed<P: Point>(&self, entry: &Entry, kind: FailureKind) -> Failure {
        let failure = Failure {
            point: P::ID,
            listener: entry.id.clone(),
            owner: entry.opts.owner.clone(),
            kind,
        };
        entry.failures.fetch_add(1, Ordering::Relaxed);
        *entry
            .last_failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(failure.to_string());
        let sinks: Vec<Sink> = self
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
fn call<P: Point>(
    entry: &Entry,
    run: &ListenerFn<P>,
    input: &Arc<P::Input>,
    value: P::Value,
    deadline: Duration,
) -> Result<P::Value, FailureKind> {
    entry.runs.fetch_add(1, Ordering::Relaxed);
    if !entry.opts.threaded {
        let start = Instant::now();
        let result = guarded::<P>(run, input, value);
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
    let run = run.clone();
    let input = input.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("hook {} {}", P::ID, entry.id))
        .spawn(move || {
            // The emit may have stopped waiting: then nobody receives.
            let _ = tx.send(guarded::<P>(&run, &input, value));
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

fn guarded<P: Point>(
    run: &ListenerFn<P>,
    input: &P::Input,
    mut value: P::Value,
) -> Result<P::Value, FailureKind> {
    match catch_unwind(AssertUnwindSafe(|| run(input, &mut value))) {
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
