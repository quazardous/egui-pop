# The hook kernel

The contract of the family "Hook kernel"
([ADR 0027](adr/0027-component-families.md)): `egui-pop-hooks`, a small
crate without dependencies and without egui, for named points where
several parts of a program contribute to one result, in an order they
declare, and where one failing contribution never breaks the others.
Its README has the full examples; this page is its contract.

It is not the application API's `alter` hooks and steps
(`egui_pop_api::hooks`, `egui_pop_api::steps`, ADR 0011), which are
tied to `App`: the kernel works in any Rust program, the interface
thread or not. No egui-pop crate depends on it yet; applications use it
for their own extension points.

[ADR 0029](adr/0029-not-on-the-bus.md) decides its first use inside
egui-pop: the application's central bus is the steps (requests), the
journal (events) and this kernel for the listeners of each, named and
run in the order they declare, the window lifecycle first. The kernel
itself stays no bus: it carries nothing and keeps no record. That ADR
adds one constraint to it, "last" (an invariants listener after every
other of its point), in its first slice.

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
  `Kernel::unlisten` removes one, `Kernel::forget` every listener of an
  owner.
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
threads. The README's examples run as doc tests.
