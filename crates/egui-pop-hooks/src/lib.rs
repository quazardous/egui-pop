#![doc = include_str!("../README.md")]
#![doc = "\n\n<div class=\"warning\">Experimental: may change in any release\n(docs/stability.md).</div>"]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod order;
mod timing;

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::thread::{self, ThreadId};
use std::time::Duration;
// `std::time::Instant::now` panics on wasm32-unknown-unknown; web-time
// is the same type on native and the browser's clock on the web.
use web_time::Instant;

/// The bus contract this kernel implements; its types are re-exported
/// below under their old paths.
pub use bus_contract;
pub use bus_contract::{
    Altered, Capability, Collected, DEFAULT_DEADLINE, Defer, FAILED_FACT, Failure, FailureKind,
    HookError, LAST, ListenOpts, ListenerInfo, ListenerTimings, OVER_BUDGET_FACT, OverBudget,
    PLUGIN_DEADLINE, PendingRun, Point, PointDecl, PointInfo, PointKind, PointSpec, PointTimings,
    Refusal, Refused, TimingStats, Timings, declare_all, missing_targets, offers, order_dump,
};
use bus_contract::{
    AnyInput, AnyListener, AnyLocalListener, AnyValue, BudgetSink, Bus, CloneFn, FailureSink,
    FreshFn, PendingWaker, PointKey,
};
use timing::Stats;

thread_local! {
    /// The local listeners registered on this thread, by kernel and
    /// listener sequence: a kernel is shared between threads, a local
    /// listener is not.
    static LOCAL: RefCell<HashMap<(u64, u64), AnyLocalListener>> = RefCell::new(HashMap::new());
}

/// Kernel ids, unique in the process (the key of [`LOCAL`]).
static NEXT_KERNEL: AtomicU64 = AtomicU64::new(0);

/// A listener's erased body, shared or local.
type ListenerFn = dyn Fn(&(dyn Any + Send + Sync), &mut (dyn Any + Send)) -> Result<(), HookError>;

/// How a listener runs: shared between threads, or local to the thread
/// that registered it (its closure in that thread's [`LOCAL`]).
#[derive(Clone)]
enum Run {
    Shared(AnyListener),
    Local(ThreadId),
}

/// What a queued emit hands its deferred listeners.
enum Work {
    /// A copy of the value the emit returned, passed along.
    Alter(AnyValue, CloneFn),
    /// A fresh value for each.
    Collect(FreshFn),
}

/// An emit's local part, queued for its thread's
/// [`Bus::run_pending`].
struct Pending {
    thread: ThreadId,
    point: &'static str,
    input: AnyInput,
    work: Work,
    entries: Vec<Arc<Entry>>,
    deadline: Duration,
    rules: Rules,
    timing: Arc<PointTiming>,
    budget: Option<Duration>,
}

/// A request asked through [`Defer`], waiting for its thread's
/// [`Defer::drain`].
struct Asked {
    thread: ThreadId,
    ask: u64,
    point: PointKey,
    input: AnyInput,
    value: AnyValue,
    clone: CloneFn,
}

/// What a point's kind and declaration make of a listener's refusal or
/// failure.
#[derive(Clone, Copy, Debug, Default)]
struct Rules {
    /// A request: a refusal stops it.
    request: bool,
    /// A request declared fail-closed: a failure stops it too.
    fail_closed: bool,
}

/// A point's listeners in order, the point's deadline, the failures
/// known before running, its rules, its passes' times and its budget.
struct Snapshot {
    entries: Vec<Arc<Entry>>,
    deadline: Duration,
    failures: Vec<Failure>,
    rules: Rules,
    timing: Arc<PointTiming>,
    budget: Option<Duration>,
}

/// A point's passes: their times and how many went over its budget.
#[derive(Default)]
struct PointTiming {
    passes: Stats,
    over: AtomicU64,
}

impl fmt::Debug for PointTiming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PointTiming").finish_non_exhaustive()
    }
}

/// One pass of a point's listeners on this thread, timed as it runs.
struct Pass {
    took: Duration,
    ran: bool,
    /// Each listener's time, kept only for a point with a budget (what an
    /// [`OverBudget`] names).
    listeners: Option<Vec<(String, Duration)>>,
}

impl Pass {
    fn new(budget: Option<Duration>) -> Pass {
        Pass {
            took: Duration::ZERO,
            ran: false,
            listeners: budget.map(|_| Vec::new()),
        }
    }

    fn add(&mut self, id: &str, took: Duration) {
        self.took += took;
        self.ran = true;
        if let Some(l) = &mut self.listeners {
            l.push((id.to_owned(), took));
        }
    }
}

/// What a listener answered instead of a value.
enum Answer {
    /// It refused the request ([`HookError::Refuse`]).
    Refuse(String),
    /// It failed.
    Fail(FailureKind),
}

/// What a listener's answer means for the emit.
enum Judged {
    /// Its contribution is dropped; the others run.
    Dropped(Failure),
    /// The request stops here, with the failure that stopped it on a
    /// fail-closed point.
    Stop(Refusal, Option<Failure>),
}

/// An alter emit's result and the listeners that ran on this thread.
type AlterRun = (Altered<AnyValue>, Vec<String>);

struct Entry {
    id: String,
    opts: ListenOpts,
    seq: u64,
    run: Run,
    /// Unlistened or forgotten: a queued run skips it.
    removed: AtomicBool,
    runs: AtomicU64,
    failures: AtomicU64,
    last_failure: Mutex<Option<String>>,
    /// Its runs' times ([`Timings`]).
    timing: Stats,
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
    /// Its passes' times ([`Timings`]).
    timing: Arc<PointTiming>,
}

impl Slot {
    fn new(key: &PointKey) -> Slot {
        Slot {
            type_id: key.type_id,
            kind: key.kind,
            spec: None,
            ordered: Vec::new(),
            missing: Vec::new(),
            timing: Arc::default(),
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
    /// Its key in each thread's local listeners.
    id: u64,
    points: RwLock<BTreeMap<&'static str, Slot>>,
    sinks: RwLock<Vec<FailureSink>>,
    wakers: RwLock<Vec<PendingWaker>>,
    /// Local parts of emits made on another thread than theirs.
    pending: Mutex<Vec<Pending>>,
    /// Requests asked through [`Defer`], for their thread's drain.
    asked: Mutex<Vec<Asked>>,
    next_ask: AtomicU64,
    /// Local listeners removed from another thread than theirs: their
    /// closures are dropped on their thread, at its next local call.
    dropped: Mutex<Vec<(ThreadId, u64)>>,
    default_deadline: Duration,
    seq: AtomicU64,
    /// Whether listener runs are timed ([`Kernel::set_timing`]).
    timing: AtomicBool,
    budget_sinks: RwLock<Vec<BudgetSink>>,
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
            id: NEXT_KERNEL.fetch_add(1, Ordering::Relaxed),
            points: RwLock::new(BTreeMap::new()),
            sinks: RwLock::new(Vec::new()),
            wakers: RwLock::new(Vec::new()),
            pending: Mutex::new(Vec::new()),
            asked: Mutex::new(Vec::new()),
            next_ask: AtomicU64::new(0),
            dropped: Mutex::new(Vec::new()),
            default_deadline: deadline,
            seq: AtomicU64::new(0),
            timing: AtomicBool::new(true),
            budget_sinks: RwLock::new(Vec::new()),
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
        self.add(key, id, opts, Run::Shared(run)).map(|_| ())
    }

    /// Registers a local listener on `P` under `id`, bound to the calling
    /// thread: it may hold state that is not `Send`. It takes its place
    /// in the point's order; an emit from another thread queues it for
    /// this thread's [`Kernel::run_pending`].
    pub fn listen_local<P: Point>(
        &self,
        id: impl Into<String>,
        opts: ListenOpts,
        run: impl Fn(&P::Input, &mut P::Value) -> Result<(), HookError> + 'static,
    ) -> Result<(), Refused> {
        self.listen_local_any(
            PointKey::of::<P>(),
            id.into(),
            opts,
            bus_contract::erase_local::<P>(run),
        )
    }

    /// Registers an erased local listener by its point's key
    /// ([`Bus::listen_local_any`]).
    pub fn listen_local_any(
        &self,
        key: PointKey,
        id: String,
        opts: ListenOpts,
        run: AnyLocalListener,
    ) -> Result<(), Refused> {
        if opts.threaded {
            return Err(Refused::Threaded {
                point: key.id,
                listener: id,
            });
        }
        self.drop_removed_locals();
        // The closure goes in first: an emit on another thread may run
        // as soon as the entry is in its slot, and only queues it.
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        LOCAL.with(|l| l.borrow_mut().insert((self.id, seq), run));
        let added = self.add_with_seq(key, id, opts, Run::Local(thread::current().id()), seq);
        if added.is_err() {
            LOCAL.with(|l| l.borrow_mut().remove(&(self.id, seq)));
        }
        added
    }

    fn add(&self, key: PointKey, id: String, opts: ListenOpts, run: Run) -> Result<(), Refused> {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        self.add_with_seq(key, id, opts, run, seq)
    }

    fn add_with_seq(
        &self,
        key: PointKey,
        id: String,
        opts: ListenOpts,
        run: Run,
        seq: u64,
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
            seq,
            run,
            removed: AtomicBool::new(false),
            runs: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            last_failure: Mutex::new(None),
            timing: Stats::default(),
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
        let (kept, gone): (Vec<_>, Vec<_>) = slot.ordered.iter().cloned().partition(|e| e.id != id);
        // Removing a listener cannot close a cycle.
        let _ = slot.set(kept);
        drop(points);
        self.removed(&gone);
        !gone.is_empty()
    }

    /// Marks removed entries so a queued run skips them, and drops the
    /// closures of local ones (now on this thread, later on another).
    fn removed(&self, gone: &[Arc<Entry>]) {
        let here = thread::current().id();
        for entry in gone {
            entry.removed.store(true, Ordering::Relaxed);
            if let Run::Local(thread) = entry.run {
                if thread == here {
                    let _ = LOCAL.try_with(|l| l.borrow_mut().remove(&(self.id, entry.seq)));
                } else {
                    self.dropped
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((thread, entry.seq));
                }
            }
        }
    }

    /// Drops this thread's closures of local listeners removed from
    /// another thread.
    fn drop_removed_locals(&self) {
        let here = thread::current().id();
        let mine: Vec<u64> = {
            let mut dropped = self.dropped.lock().unwrap_or_else(PoisonError::into_inner);
            if dropped.is_empty() {
                return;
            }
            let (mine, rest) = std::mem::take(&mut *dropped)
                .into_iter()
                .partition(|(t, _)| *t == here);
            *dropped = rest;
            mine.into_iter().map(|(_, seq)| seq).collect()
        };
        LOCAL.with(|l| {
            let mut l = l.borrow_mut();
            for seq in mine {
                l.remove(&(self.id, seq));
            }
        });
    }

    /// Removes every listener of `owner`, on every point (a plugin
    /// uninstalled, a module switched off); how many were removed.
    pub fn forget(&self, owner: &str) -> usize {
        let mut points = self.write();
        let mut gone = Vec::new();
        for slot in points.values_mut() {
            let (kept, out): (Vec<_>, Vec<_>) = slot
                .ordered
                .iter()
                .cloned()
                .partition(|e| e.opts.owner.as_deref() != Some(owner));
            if !out.is_empty() {
                gone.extend(out);
                let _ = slot.set(kept);
            }
        }
        drop(points);
        self.removed(&gone);
        gone.len()
    }

    /// Adds a waker called, with the thread's id, when an emit queues
    /// local listeners for that thread. A waker that panics is ignored.
    pub fn on_pending(&self, wake: impl Fn(ThreadId) + Send + Sync + 'static) {
        self.add_pending_waker(Arc::new(wake));
    }

    fn add_pending_waker(&self, wake: PendingWaker) {
        self.wakers
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(wake);
    }

    /// Queues an emit's deferred local listeners, one queued emit per
    /// thread, and calls the wakers.
    fn defer(
        &self,
        point: &'static str,
        input: &AnyInput,
        deferred: Vec<Arc<Entry>>,
        snap: (Duration, Rules, &Arc<PointTiming>, Option<Duration>),
        work: impl Fn() -> Work,
    ) {
        let (deadline, rules, timing, budget) = snap;
        let mut threads: Vec<ThreadId> = Vec::new();
        for e in &deferred {
            if let Run::Local(t) = e.run
                && !threads.contains(&t)
            {
                threads.push(t);
            }
        }
        for thread in threads {
            let entries = deferred
                .iter()
                .filter(|e| matches!(e.run, Run::Local(t) if t == thread))
                .cloned()
                .collect();
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Pending {
                    thread,
                    point,
                    input: input.clone(),
                    work: work(),
                    entries,
                    deadline,
                    rules,
                    timing: timing.clone(),
                    budget,
                });
            self.wake(thread);
        }
    }

    /// Calls the wakers for `thread`; one that panics is ignored.
    fn wake(&self, thread: ThreadId) {
        let wakers: Vec<PendingWaker> = self
            .wakers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for wake in wakers {
            let _ = catch_unwind(AssertUnwindSafe(|| wake(thread)));
        }
    }

    /// Runs the local listeners queued for the calling thread by emits on
    /// other threads, emit by emit in the order they were queued
    /// ([`Bus::run_pending`]). A listener removed since is skipped.
    pub fn run_pending(&self) -> Vec<PendingRun> {
        self.drop_removed_locals();
        let here = thread::current().id();
        let mine: Vec<Pending> = {
            let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
            if pending.is_empty() {
                return Vec::new();
            }
            let (mine, rest) = std::mem::take(&mut *pending)
                .into_iter()
                .partition(|p| p.thread == here);
            *pending = rest;
            mine
        };
        mine.into_iter().map(|p| self.run_queued(p)).collect()
    }

    fn run_queued(&self, p: Pending) -> PendingRun {
        let mut ran = Vec::new();
        let mut failures = Vec::new();
        let mut refused = None;
        let mut pass = Pass::new(p.budget);
        let (mut value, fresh) = match p.work {
            Work::Alter(value, clone) => (Some((value, clone)), None),
            Work::Collect(fresh) => (None, Some(fresh)),
        };
        for entry in &p.entries {
            if entry.removed.load(Ordering::Relaxed) {
                continue;
            }
            let v = match (&value, fresh) {
                (Some((v, clone)), _) => clone(&**v),
                (None, Some(fresh)) => fresh(),
                (None, None) => continue,
            };
            ran.push(entry.id.clone());
            match self.call(
                p.point,
                entry,
                &p.input,
                v,
                entry.opts.deadline.unwrap_or(p.deadline),
                &mut pass,
            ) {
                Some(Ok(v)) => {
                    if let Some((old, _)) = &mut value {
                        *old = v;
                    }
                }
                Some(Err(answer)) => match self.judge(p.point, entry, answer, p.rules) {
                    Judged::Dropped(f) => failures.push(f),
                    Judged::Stop(r, f) => {
                        failures.extend(f);
                        refused = Some(r);
                        break;
                    }
                },
                None => {
                    ran.pop();
                }
            }
        }
        self.end_pass(p.point, &p.timing, p.budget, pass);
        let mut run = PendingRun::new(p.point);
        run.ran = ran;
        run.failures = failures;
        run.refused = refused;
        run
    }

    /// Counts a pass that ran something in its point's times and reports
    /// it to the budget sinks when it went over the budget.
    fn end_pass(
        &self,
        point: &'static str,
        timing: &PointTiming,
        budget: Option<Duration>,
        pass: Pass,
    ) {
        if !pass.ran || !self.timing.load(Ordering::Relaxed) {
            return;
        }
        timing.passes.record(pass.took);
        let Some(budget) = budget else { return };
        if pass.took <= budget {
            return;
        }
        timing.over.fetch_add(1, Ordering::Relaxed);
        let sinks: Vec<BudgetSink> = self
            .budget_sinks
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if sinks.is_empty() {
            return;
        }
        let mut over = OverBudget::new(point, pass.took, budget);
        over.listeners = pass.listeners.unwrap_or_default();
        for sink in sinks {
            let _ = catch_unwind(AssertUnwindSafe(|| sink(&over)));
        }
    }

    /// What a listener's refusal or failure means under `rules`; a
    /// failure is counted and sent to the sinks.
    fn judge(&self, point: &'static str, entry: &Entry, answer: Answer, rules: Rules) -> Judged {
        let kind = match answer {
            Answer::Refuse(reason) if rules.request => {
                let refusal =
                    Refusal::new(point, entry.id.clone(), entry.opts.owner.clone(), reason);
                return Judged::Stop(refusal, None);
            }
            Answer::Refuse(reason) => FailureKind::Error(reason),
            Answer::Fail(kind) => kind,
        };
        let failure = self.failed(point, entry, kind);
        if rules.fail_closed {
            Judged::Stop(Refusal::from_failure(&failure), Some(failure))
        } else {
            Judged::Dropped(failure)
        }
    }

    /// Asks the request `P` now: its listeners run at the calling
    /// thread's next [`Kernel::drain`] ([`Defer`]). The ask's number.
    pub fn ask<P: Point>(&self, input: P::Input, value: P::Value) -> u64
    where
        P::Value: Clone,
    {
        (self as &dyn Defer).ask::<P>(input, value)
    }

    /// Queues an alter emit of `point` for the calling thread's next
    /// [`Kernel::drain`] and calls the wakers ([`Defer::ask_any`]).
    pub fn ask_any(
        &self,
        point: PointKey,
        input: AnyInput,
        value: AnyValue,
        clone: CloneFn,
    ) -> u64 {
        let ask = self.next_ask.fetch_add(1, Ordering::Relaxed);
        let thread = thread::current().id();
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Asked {
                thread,
                ask,
                point,
                input,
                value,
                clone,
            });
        self.wake(thread);
        ask
    }

    /// Runs the requests asked on the calling thread, in ask order, each
    /// as an alter emit with the listeners registered now; what each did
    /// ([`Defer::drain`]). An ask made while draining waits for the
    /// next drain.
    pub fn drain(&self) -> Vec<PendingRun> {
        let here = thread::current().id();
        let mine: Vec<Asked> = {
            let mut asked = self.asked.lock().unwrap_or_else(PoisonError::into_inner);
            if asked.is_empty() {
                return Vec::new();
            }
            let (mine, rest) = std::mem::take(&mut *asked)
                .into_iter()
                .partition(|a| a.thread == here);
            *asked = rest;
            mine
        };
        mine.into_iter()
            .map(|a| {
                let (out, ran) = self.alter_erased(a.point, a.input, a.value, a.clone);
                let mut run = PendingRun::new(a.point.id);
                run.ran = ran;
                run.failures = out.failures;
                run.refused = out.refused;
                run.ask = Some(a.ask);
                run
            })
            .collect()
    }

    /// How many requests asked on the calling thread wait for its drain.
    pub fn waiting(&self) -> usize {
        let here = thread::current().id();
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|a| a.thread == here)
            .count()
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
    ) -> AlterRun {
        let Snapshot {
            entries,
            deadline,
            mut failures,
            rules,
            timing,
            budget,
        } = self.snapshot(&key);
        let mut pass = Pass::new(budget);
        let mut value = value;
        let mut deferred = Vec::new();
        let mut ran = Vec::new();
        let mut refused = None;
        for entry in &entries {
            if entry.elsewhere() {
                deferred.push(entry.clone());
                continue;
            }
            let answer = self.call(
                key.id,
                entry,
                &input,
                clone(&*value),
                entry.opts.deadline.unwrap_or(deadline),
                &mut pass,
            );
            if answer.is_some() {
                ran.push(entry.id.clone());
            }
            match answer {
                Some(Ok(v)) => value = v,
                Some(Err(answer)) => match self.judge(key.id, entry, answer, rules) {
                    Judged::Dropped(f) => failures.push(f),
                    Judged::Stop(r, f) => {
                        failures.extend(f);
                        refused = Some(r);
                        break;
                    }
                },
                None => {}
            }
        }
        self.end_pass(key.id, &timing, budget, pass);
        if refused.is_some() {
            // A refused request queues nothing for other threads.
            deferred.clear();
        }
        let ids = deferred.iter().map(|e| e.id.clone()).collect();
        if !deferred.is_empty() {
            let snap = (deadline, rules, &timing, budget);
            self.defer(key.id, &input, deferred, snap, || {
                Work::Alter(clone(&*value), clone)
            });
        }
        let mut out = Altered::new(value);
        out.failures = failures;
        out.deferred = ids;
        out.refused = refused;
        (out, ran)
    }

    fn collect_erased(
        &self,
        key: PointKey,
        input: AnyInput,
        fresh: FreshFn,
    ) -> Collected<AnyValue> {
        let Snapshot {
            entries,
            deadline,
            failures,
            rules,
            timing,
            budget,
        } = self.snapshot(&key);
        let mut pass = Pass::new(budget);
        let mut out = Collected::new();
        out.failures = failures;
        let mut deferred = Vec::new();
        for entry in &entries {
            if entry.elsewhere() {
                deferred.push(entry.clone());
                continue;
            }
            match self.call(
                key.id,
                entry,
                &input,
                fresh(),
                entry.opts.deadline.unwrap_or(deadline),
                &mut pass,
            ) {
                Some(Ok(v)) => out.parts.push((entry.id.clone(), v)),
                Some(Err(answer)) => match self.judge(key.id, entry, answer, rules) {
                    Judged::Dropped(f) => out.failures.push(f),
                    Judged::Stop(r, f) => {
                        out.failures.extend(f);
                        out.refused = Some(r);
                        deferred.clear();
                        break;
                    }
                },
                None => {}
            }
        }
        self.end_pass(key.id, &timing, budget, pass);
        out.deferred = deferred.iter().map(|e| e.id.clone()).collect();
        if !deferred.is_empty() {
            let snap = (deadline, rules, &timing, budget);
            self.defer(key.id, &input, deferred, snap, || Work::Collect(fresh));
        }
        out
    }

    /// The listeners of a point in order, the point's deadline, and the
    /// failures known before running (missing targets, a type mismatch).
    /// Taken under the lock, used without it.
    fn snapshot(&self, key: &PointKey) -> Snapshot {
        let points = self.read();
        let default = self.default_deadline;
        let empty = |failures| Snapshot {
            entries: Vec::new(),
            deadline: default,
            failures,
            rules: Rules::default(),
            timing: Arc::default(),
            budget: None,
        };
        let Some(slot) = points.get(key.id) else {
            return empty(Vec::new());
        };
        if slot.type_id != key.type_id {
            let failure = Failure::new(key.id, "*", None, FailureKind::TypeMismatch);
            return empty(vec![failure]);
        }
        let request = slot.kind == PointKind::Request;
        let rules = Rules {
            request,
            fail_closed: request && slot.spec.as_ref().is_some_and(|s| s.fail_closed),
        };
        let point_deadline = slot
            .spec
            .as_ref()
            .and_then(|s| s.deadline)
            .unwrap_or(default);
        let failures = slot
            .missing
            .iter()
            .map(|(listener, target)| {
                let owner = slot
                    .ordered
                    .iter()
                    .find(|e| &e.id == listener)
                    .and_then(|e| e.opts.owner.clone());
                Failure::new(
                    key.id,
                    listener.clone(),
                    owner,
                    FailureKind::MissingTarget(target.clone()),
                )
            })
            .collect();
        Snapshot {
            entries: slot.ordered.clone(),
            deadline: point_deadline,
            failures,
            rules,
            timing: slot.timing.clone(),
            budget: slot.spec.as_ref().and_then(|s| s.budget),
        }
    }

    fn failed(&self, point: &'static str, entry: &Entry, kind: FailureKind) -> Failure {
        let failure = Failure::new(point, entry.id.clone(), entry.opts.owner.clone(), kind);
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
        let mut info = PointInfo::new(id, slot.kind);
        if let Some(spec) = &slot.spec {
            info.description = spec.description.clone();
            info.declared = true;
            info.fail_closed = spec.fail_closed;
            info.budget = spec.budget;
        }
        info.deadline = deadline;
        info.listeners = slot
            .ordered
            .iter()
            .map(|e| {
                let mut l = ListenerInfo::new(e.id.clone());
                l.owner = e.opts.owner.clone();
                l.after = e.opts.after.clone();
                l.before = e.opts.before.clone();
                l.priority = e.opts.priority;
                l.deadline = e.opts.deadline.unwrap_or(deadline);
                l.threaded = e.opts.threaded;
                l.local = matches!(e.run, Run::Local(_));
                l.missing = slot
                    .missing
                    .iter()
                    .filter(|(l, _)| *l == e.id)
                    .map(|(_, t)| t.clone())
                    .collect();
                l.runs = e.runs.load(Ordering::Relaxed);
                l.failures = e.failures.load(Ordering::Relaxed);
                l.last_failure = e
                    .last_failure
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                l
            })
            .collect();
        info
    }
}

impl Entry {
    /// A local listener bound to another thread than the caller's.
    fn elsewhere(&self) -> bool {
        matches!(self.run, Run::Local(t) if t != thread::current().id())
    }
}

impl Kernel {
    /// Runs one listener on `value`, on this thread or its own, within
    /// its deadline, panics caught; `None` for a local listener whose
    /// closure this thread does not hold (removed meanwhile).
    ///
    /// Its time goes to the listener's times and to `pass`.
    fn call(
        &self,
        point: &'static str,
        entry: &Entry,
        input: &AnyInput,
        value: AnyValue,
        deadline: Duration,
        pass: &mut Pass,
    ) -> Option<Result<AnyValue, Answer>> {
        let (result, took) = match &entry.run {
            Run::Shared(run) => {
                entry.runs.fetch_add(1, Ordering::Relaxed);
                call(point, entry, run, input, value, deadline)
            }
            Run::Local(_) => {
                // Taken out of the cell first: the listener may emit or
                // listen.
                let run = LOCAL.with(|l| l.borrow().get(&(self.id, entry.seq)).cloned())?;
                entry.runs.fetch_add(1, Ordering::Relaxed);
                timed(&*run, input, value, deadline)
            }
        };
        if self.timing.load(Ordering::Relaxed) {
            entry.timing.record(took);
            pass.add(&entry.id, took);
        }
        Some(result)
    }

    /// Times listener runs (the default) or stops: when off, nothing is
    /// counted and no pass is reported over budget; the deadlines still
    /// hold. Turning it off keeps the times so far.
    pub fn set_timing(&self, on: bool) {
        self.timing.store(on, Ordering::Relaxed);
    }

    /// Whether listener runs are timed ([`Kernel::set_timing`]).
    pub fn timing(&self) -> bool {
        self.timing.load(Ordering::Relaxed)
    }

    /// Every point that ran a pass since the last reset, in id order, its
    /// listeners in the order they run ([`Timings::points`]).
    pub fn timings(&self) -> Vec<PointTimings> {
        self.read()
            .iter()
            .filter_map(|(id, slot)| point_timings(id, slot))
            .collect()
    }

    /// The timings of the point `point` ([`Timings::point`]).
    pub fn point_timings(&self, point: &str) -> Option<PointTimings> {
        let points = self.read();
        let (id, slot) = points.get_key_value(point)?;
        point_timings(id, slot)
    }

    /// Starts a new window of times ([`Timings::reset`]).
    pub fn reset_timings(&self) {
        for slot in self.read().values() {
            slot.timing.passes.reset();
            slot.timing.over.store(0, Ordering::Relaxed);
            for e in &slot.ordered {
                e.timing.reset();
            }
        }
    }

    /// Adds a sink called with each pass over its point's budget, on the
    /// thread that ran it ([`Timings::add_budget_sink`]). A sink that
    /// panics is ignored.
    pub fn on_over_budget(&self, sink: impl Fn(&OverBudget) + Send + Sync + 'static) {
        self.add_budget_sink(Arc::new(sink));
    }

    fn add_budget_sink(&self, sink: BudgetSink) {
        self.budget_sinks
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(sink);
    }
}

impl Drop for Kernel {
    /// Drops this thread's local listeners of the kernel; another
    /// thread's go when that thread ends.
    fn drop(&mut self) {
        let id = self.id;
        let _ = LOCAL.try_with(|l| l.borrow_mut().retain(|(k, _), _| *k != id));
    }
}

/// Runs a listener inline, late once it returns past its deadline.
fn timed(
    run: &ListenerFn,
    input: &AnyInput,
    value: AnyValue,
    deadline: Duration,
) -> (Result<AnyValue, Answer>, Duration) {
    let start = Instant::now();
    let result = guarded(run, &**input, value);
    let took = start.elapsed();
    let result = match result {
        Ok(_) if took > deadline => Err(Answer::Fail(FailureKind::Late {
            deadline,
            took: Some(took),
        })),
        r => r,
    };
    (result, took)
}

/// Runs one shared listener on `value`, inline or on its own thread,
/// within its deadline, panics caught; with the time it took (for a
/// threaded one, the time the emit waited).
fn call(
    point: &'static str,
    entry: &Entry,
    run: &AnyListener,
    input: &AnyInput,
    value: AnyValue,
    deadline: Duration,
) -> (Result<AnyValue, Answer>, Duration) {
    if !entry.opts.threaded {
        return timed(&**run, input, value, deadline);
    }
    let start = Instant::now();
    let result = threaded(point, entry, run, input, value, deadline);
    (result, start.elapsed())
}

/// Runs one shared listener on a thread of its own, waiting for it up to
/// its deadline.
fn threaded(
    point: &'static str,
    entry: &Entry,
    run: &AnyListener,
    input: &AnyInput,
    value: AnyValue,
    deadline: Duration,
) -> Result<AnyValue, Answer> {
    let (tx, rx) = sync_channel(1);
    let run = run.clone();
    let input = input.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("hook {point} {}", entry.id))
        .spawn(move || {
            // The emit may have stopped waiting: then nobody receives.
            let _ = tx.send(guarded(&*run, &*input, value));
        });
    if let Err(e) = spawned {
        return Err(Answer::Fail(FailureKind::Error(format!(
            "could not start its thread: {e}"
        ))));
    }
    match rx.recv_timeout(deadline) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(Answer::Fail(FailureKind::Late {
            deadline,
            took: None,
        })),
        Err(RecvTimeoutError::Disconnected) => Err(Answer::Fail(FailureKind::Panic(
            "its thread ended without an answer".to_owned(),
        ))),
    }
}

fn guarded(
    run: &ListenerFn,
    input: &(dyn Any + Send + Sync),
    mut value: AnyValue,
) -> Result<AnyValue, Answer> {
    match catch_unwind(AssertUnwindSafe(|| run(input, &mut *value))) {
        Ok(Ok(())) => Ok(value),
        Ok(Err(HookError::Refuse(reason))) => Err(Answer::Refuse(reason)),
        Ok(Err(e)) => Err(Answer::Fail(FailureKind::Error(e.message().to_owned()))),
        Err(payload) => Err(Answer::Fail(FailureKind::Panic(panic_message(
            payload.as_ref(),
        )))),
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

    fn listen_local_any(
        &self,
        point: PointKey,
        id: String,
        opts: ListenOpts,
        run: AnyLocalListener,
    ) -> Result<(), Refused> {
        Kernel::listen_local_any(self, point, id, opts, run)
    }

    fn run_pending(&self) -> Vec<PendingRun> {
        Kernel::run_pending(self)
    }

    fn add_pending_waker(&self, wake: PendingWaker) {
        Kernel::add_pending_waker(self, wake);
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
        self.alter_erased(point, input, value, clone).0
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

    fn defer(&self) -> Option<&dyn Defer> {
        Some(self)
    }

    fn timings(&self) -> Option<&dyn Timings> {
        Some(self)
    }
}

impl Timings for Kernel {
    fn points(&self) -> Vec<PointTimings> {
        Kernel::timings(self)
    }

    fn point(&self, point: &str) -> Option<PointTimings> {
        Kernel::point_timings(self, point)
    }

    fn reset(&self) {
        Kernel::reset_timings(self);
    }

    fn add_budget_sink(&self, sink: BudgetSink) {
        Kernel::add_budget_sink(self, sink);
    }
}

/// A point's times, `None` when it ran no pass since the last reset.
fn point_timings(id: &'static str, slot: &Slot) -> Option<PointTimings> {
    if slot.timing.passes.count() == 0 {
        return None;
    }
    let mut out = PointTimings::new(id);
    out.budget = slot.spec.as_ref().and_then(|s| s.budget);
    out.passes = slot.timing.passes.read();
    out.over_budget = slot.timing.over.load(Ordering::Relaxed);
    out.listeners = slot
        .ordered
        .iter()
        .filter(|e| e.timing.count() > 0)
        .map(|e| {
            let mut l = ListenerTimings::new(e.id.clone());
            l.owner = e.opts.owner.clone();
            l.stats = e.timing.read();
            l
        })
        .collect();
    Some(out)
}

impl Defer for Kernel {
    fn ask_any(&self, point: PointKey, input: AnyInput, value: AnyValue, clone: CloneFn) -> u64 {
        Kernel::ask_any(self, point, input, value, clone)
    }

    fn drain(&self) -> Vec<PendingRun> {
        Kernel::drain(self)
    }

    fn waiting(&self) -> usize {
        Kernel::waiting(self)
    }
}
