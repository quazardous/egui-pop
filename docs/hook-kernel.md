# The hook kernel

The contract of the family "Hook kernel"
([ADR 0027](adr/0027-component-families.md)): `egui-pop-hooks`, a small
crate without egui whose only dependency is the bus contract
([`bus-contract`](https://github.com/quazardous/bus-contract), no
dependencies of its own), for named points where
several parts of a program contribute to one result, in an order they
declare, and where one failing contribution never breaks the others.
Its README has the full examples; this page is its contract.

It is not the application API's `alter` hooks and steps
(`egui_pop_api::hooks`, `egui_pop_api::steps`, ADR 0011), which are
tied to `App`: the kernel works in any Rust program, the interface
thread or not. Applications use it for their own extension points.

[ADR 0029](adr/0029-not-on-the-bus.md) makes it part of egui-pop's own
central bus: the steps (requests), the journal (events) and this kernel
for the listeners of each, named and run in the order they declare, the
window lifecycle first. `egui-pop-api` depends on it: each application
holds a kernel (`AppHandle::bus`) where the window points
(`egui_pop_api::bus`) and their listeners live. The kernel itself stays
no bus: it carries nothing and keeps no record. That ADR added one
constraint to it, "last" (an invariants listener after every other of
its point).

[ADR 0030](adr/0030-the-bus-is-a-contract.md) makes the bus a contract
the application supplies: the traits and types live in the external
`bus-contract` crate (re-exported here), and `Kernel` is its default
implementation, the one `egui-pop-api` builds when the application gives
it no bus (`AppBuilder::bus`). A library never constructs a kernel
(`cargo xtask bus --check`); an application may, or write its own
implementation of `bus_contract::Bus`.

**Owns**: typed hook points, listeners with ids ordered by
`after`/`before` and priority, `alter` and `collect` emits, a
per-listener error boundary (errors, caught panics, deadlines), failure
sinks, introspection.

**Refuses**: UI of any kind; event buses, and anything across processes
(a listener answering from another process is the caller's transport,
given a longer deadline).

## The API

```rust
use egui_pop_hooks::{Kernel, ListenOpts, Point, PointSpec};

/// The findings of a health report: each check adds its own.
struct HealthReport;
impl Point for HealthReport {
    const ID: &'static str = "health.report";
    type Input = ();
    type Value = Vec<String>;
}

let kernel = Kernel::new();
kernel.declare::<HealthReport>(PointSpec::new("The findings of the health report")).unwrap();
kernel
    .listen::<HealthReport>("storage.free-space", ListenOpts::new().owner("storage"), |_, out| {
        out.push("disk 91% full".into());
        Ok(())
    })
    .unwrap();
let report = kernel.collect::<HealthReport>(());
assert!(report.failures.is_empty());
```

- **A point is a type** (`egui_pop_hooks::Point`): its id, the `Input`
  every listener reads, the `Value` a listener edits. Declaring it
  (`Kernel::declare`, with a `PointSpec`) is optional and adds a
  description and a deadline for introspection.
- **Listeners** (`Kernel::listen`) have ids, unique per point. They run
  ordered by `after`/`before`, then by priority (higher first), then by
  registration order: the same listeners always run in the same order.
  `ListenOpts::last` runs one after every other listener of its point
  (its `after` holds `egui_pop_hooks::LAST`); a second one on a point
  closes a cycle and is refused. `Kernel::unlisten` removes one,
  `Kernel::forget` every listener of an owner.
- **`alter`** passes one value through the listeners in order
  (`Altered`: the value and the failures); **`collect`** gives each
  listener a fresh value and gathers each successful part with its
  listener's id (`Collected`).
- **Introspection**: `Kernel::points` and `Kernel::listeners` list the
  points, their listeners in run order, owners, constraints, deadlines,
  missing targets and counters (runs, failures, the last failure).

## Failure behaviour

- **Registration refusals** (`Refused`) change nothing: a duplicate id,
  an ordering cycle (named with the listeners in it), a point id
  registered with another type, a point declared twice.
- **Per listener**: an edit counts only when the listener returns `Ok`,
  without panicking (panics are caught), within its deadline. Otherwise
  the edit is dropped, the other listeners still run, and the emit
  returns a `Failure` (`FailureKind::Error`, `Panic`, `Late`).
- **Deadlines**: one second by default
  (`egui_pop_hooks::DEFAULT_DEADLINE`), set per point and per listener
  (`PLUGIN_DEADLINE`, five seconds, for listeners answering from another
  process); a listener's own deadline wins over its point's, which wins
  over the kernel's. An inline listener runs on the emitting thread and
  counts as late once it returns; a `threaded` one runs on a thread of
  its own and the emit stops waiting at the deadline.
- **A missing `after`/`before` target** is reported
  (`FailureKind::MissingTarget`) and ignored; the listener's part still
  counts. It is not sent to the sinks.
- **An emit on a point registered with another type** runs no listener
  and reports `FailureKind::TypeMismatch`.
- **Sinks** (`Kernel::on_failure`) receive each dropped contribution as
  it happens; a host publishes them as a `hook.failed` fact
  (`egui_pop_hooks::FAILED_FACT`) or logs them.
- Listeners may call the kernel again (re-entry), and emits may come
  from several threads: the kernel is `Send + Sync`.
- **Local listeners** (`listen_local::<P>`) hold state that is not
  `Send` (an interface's `Rc<RefCell<…>>`). Each is bound to the thread
  that registered it and takes its place in the same order as the
  others. An emit on that thread runs it inline; an emit from another
  thread runs the others, queues it (`Altered::deferred`,
  `Collected::deferred`) and calls the `on_pending` wakers; the thread
  runs the queue in order with `run_pending()`. egui-pop's application
  calls `AppHandle::run_pending` once per pass, before the `On::Pass`
  handlers, and asks for a pass when work is queued.
- **Listeners for a window** (`AppHandle::listen_local_in::<P>(window,
  …)`): a local listener registered for an application window wakes
  that window each time it runs, inline or from the queue, so what it
  changed is drawn there within one frame without a hand-written
  `request_repaint_of`. The main window is still woken to run the queue
  and the asks; the window's wake comes in addition. A window drawn in
  the main one (the main window, a detached panel's, every window where
  they are embedded: the web, kittest) or no longer open wakes the main
  window. `AppHandle::wake_window` asks a window's pass the same way.
- **Requests heard after the drawing** (the contract's `Defer`
  capability, which the kernel offers): code that cannot be re-entered
  while it draws (a panel with its state borrowed) asks a request with
  `AppHandle::ask::<P>` (or `bus.defer()?.ask::<P>`); it returns at once
  and the listeners run at the thread's next drain. egui-pop's
  application drains (`AppHandle::drain_asks`) once per pass, after the
  main window's panels, its detached panels' windows and its zones are
  drawn, so an ask made while drawing is heard in the same pass; and
  before it quits (`App::exit`: before the `on_exit` functions, and
  after them for what they asked), so no ask is lost. An ask made by a
  listener during a drain waits for the next one. A drain that ran
  something asks for a pass. An application window (a deferred
  viewport) is drawn on the same thread after the main window: its asks
  wait in the same queue, and the main window's next pass is asked for.
  `AppHandle::asks_waiting` counts what waits; on a bus without `Defer`,
  `ask` answers `None` and the caller asks at once or keeps its own
  queue.
- **Listener times and budgets** (the contract's `Timings` capability,
  which the kernel offers): every listener run is timed (the two clock
  reads the deadlines already make, then a few relaxed atomic adds into
  a log histogram of √2 steps), per listener and per pass. A pass is
  what one emit's listeners ran on one thread (an inline emit, a queued
  run, a drained ask). `bus.timings()?.points()` gives count, total,
  max, p50 and p95 (within a bucket) and `PointTimings::slowest`;
  `reset()` starts a new window; `Kernel::set_timing(false)` stops
  counting. A point may declare a budget for a pass
  (`PointDecl::budget`); a pass over it goes to the `on_over_budget`
  sinks and its results are kept: a budget is not a deadline. egui-pop's
  application says it as the `bus.overBudget` fact (point, `took_us`,
  `budget_us`, each listener's time, `slowest`, and `unsaid`: the passes
  over budget not said since) and one warning in the log, at most once a
  minute per point. The debug collector `bus` shows each point's budget
  and passes, each listener's runs, and `slowest` across points.
- **Refusals and fail-closed points**: a listener on a request returns
  `HookError::refuse(reason)` and the request stops there; on a point
  declared `fail_closed`, a listener's error, panic or missed deadline
  refuses it too. egui-pop's `command.run` (`bus::CommandRun`) is such a
  point: every command run (a key, the palette, a menu, a toolbar, an
  agent) is asked on it before its handler; a refusal stops the run,
  which never reaches the host, and is journaled as `command.refused`
  with `by` (the listener), `reason` and `failed`. With nobody listening,
  commands run as before.

## Accessibility

None: the kernel has no UI. What a host builds from its results (a
health report, a menu) follows the rules of the family that draws it.

## Tier

From [API stability](stability.md): **Experimental** ("The hook kernel
(`egui-pop-hooks`): new; its API may move while the first applications
move their extension points onto it"). The crate is not on crates.io
yet.

## Tests

`crates/egui-pop-hooks/tests/kernel.rs`: ordering by constraints,
priority and registration; cycles refused; missing targets; duplicate
ids and mismatched types; `alter` keeping only successful edits;
`collect` in order; inline and threaded deadlines and their resolution;
sinks; introspection; `forget` and `unlisten`; re-entry and emits across
threads; local listeners in order, deferred from another thread, removed
from any thread. `crates/egui-pop-api/tests/bus_defer.rs`: an ask made in a panel heard after
it in the same pass, asks made before and during exit heard, a palette
command refused by a `command.run` listener, a failing listener refusing
on the fail-closed point. `crates/egui-pop-api/tests/app_bus.rs`: a local listener
on `window.close` in its place, and run at the next pass when the
request comes from a worker. The README's examples run as doc tests.
