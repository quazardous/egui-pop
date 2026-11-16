# egui-pop-hooks

A small hook kernel: named **hook points** where several parts of a
program contribute, in an order they declare, to one result, and where
one failing contribution never breaks the others.

- **Typed points.** A point is a type: its id, the input every listener
  reads and the value a listener edits, checked at compile time.
- **Listeners with ids,** ordered by `after`/`before` first, then by
  priority (higher first), then by registration order: the same
  listeners always run in the same order. A cycle is refused when the
  listener registers, naming the listeners in it; a constraint naming a
  listener that is not there is reported and ignored.
- **Two emits on one listener model.** `alter` passes one value through
  the listeners in order, each may change it; `collect` gives each
  listener a fresh value and gathers them in a list.
- **An error boundary per listener.** A listener's edit counts only when
  it returns `Ok`, without panicking (panics are caught), within its
  deadline. Otherwise its edit is dropped, the others still run, and the
  emit returns the failure. Sinks receive each failure as it happens, so
  a host can publish a `hook.failed` fact or log it.
- **Deadlines.** One second by default, overridable per point and per
  listener (`PLUGIN_DEADLINE`, five seconds, for listeners that answer
  from another process). An inline listener runs on the emitting thread
  and counts as late once it returns; a threaded one runs on a thread of
  its own and the emit stops waiting at the deadline.
- **Introspection.** `points()` lists every point and its listeners in
  the order they run, with their owners, constraints, deadlines, missing
  targets and counters (runs, failures, the last failure), for people,
  agents and inspection endpoints.
- **An implementation of [`bus-contract`](https://github.com/quazardous/bus-contract).**
  The kernel is a `bus_contract::Bus`: a library that takes an
  `Arc<dyn Bus>` from its application runs on it, or on any other
  implementation the application prefers. The contract's types
  (`Point`, `ListenOpts`, `PointSpec`…) are re-exported here.
- One dependency (`bus-contract`, itself without any), no `unsafe`, no GUI toolkit, no async runtime: the
  kernel is `Send + Sync`, listeners are `Send + Sync` closures.

It was written for [egui-pop](https://github.com/quazardous/egui-pop)
and the applications built on it, and works in any Rust program. Its
ideas (listener ids, dependency ordering, an error boundary) come from
[QuarKernel](https://github.com/quazardous/quarkernel).

## Collect: each listener adds its part

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
kernel
    .declare::<HealthReport>(PointSpec::new("The findings of the health report"))
    .unwrap();
kernel
    .listen::<HealthReport>("storage.free-space", ListenOpts::new().owner("storage"), |_, out| {
        out.push("disk 91% full".into());
        Ok(())
    })
    .unwrap();
kernel
    .listen::<HealthReport>("gpu.driver", ListenOpts::new().owner("gpu"), |_, _| {
        Err("driver not found".into())
    })
    .unwrap();

let report = kernel.collect::<HealthReport>(());
assert_eq!(report.parts, [("storage.free-space".to_string(), vec!["disk 91% full".to_string()])]);
assert_eq!(report.failures.len(), 1);
assert_eq!(report.failures[0].listener, "gpu.driver");
```

## Alter: the value goes through the listeners in order

```rust
use egui_pop_hooks::{Kernel, ListenOpts, Point};

/// The entries of a menu, which plugins may add to or reorder.
struct FileMenu;
impl Point for FileMenu {
    const ID: &'static str = "menu.file";
    type Input = ();
    type Value = Vec<&'static str>;
}

let kernel = Kernel::new();
kernel
    .listen::<FileMenu>("sort", ListenOpts::new().after("export"), |_, items| {
        items.sort();
        Ok(())
    })
    .unwrap();
kernel
    .listen::<FileMenu>("export", ListenOpts::new(), |_, items| {
        items.push("Export");
        Ok(())
    })
    .unwrap();
kernel
    .listen::<FileMenu>("broken", ListenOpts::new().priority(10), |_, items| {
        items.clear();
        panic!("half-way through");
    })
    .unwrap();

let menu = kernel.alter::<FileMenu>((), vec!["Open", "Close"]);
// `broken` ran first (priority) and panicked: its edit was dropped.
assert_eq!(menu.value, ["Close", "Export", "Open"]);
assert_eq!(menu.failures.len(), 1);
```

## Failures as facts

```rust
use std::sync::{Arc, Mutex};
use egui_pop_hooks::{FAILED_FACT, Kernel};

let kernel = Kernel::new();
let published = Arc::new(Mutex::new(Vec::new()));
let sink = published.clone();
kernel.on_failure(move |failure| {
    // A host publishes it on its event bus.
    sink.lock().unwrap().push((FAILED_FACT, failure.to_string()));
});
```

## Next to an event bus, not on it

An event bus says what happened; nobody answers. A hook emit is a call
that builds a result and returns it with every listener's failure. A
program usually has both: the owner of a point emits, then publishes a
fact when others should know the result exists.

License: MIT OR Apache-2.0.
