# egui-pop-hooks

egui-pop's hook kernel, now [`bus-kernel`](https://crates.io/crates/bus-kernel):
this crate re-exports it, so `egui_pop_hooks::Kernel` and every other
path of 0.2 still name the same items.

`bus-kernel` is an implementation of
[`bus-contract`](https://github.com/quazardous/bus-contract): typed hook
points, listeners with ids ordered by `after`/`before` and priority,
`alter` and `collect` emits, a per-listener error boundary with panics
caught and deadlines, local listeners, deferred requests, timings, and a
host journal and relay. It needs no GUI toolkit, and ships a conformance
suite (its `conformance` feature) any other implementation can run.

New code may depend on `bus-kernel` directly. egui-pop's application
uses this kernel by default (`AppBuilder::bus` supplies another).

License: MIT OR Apache-2.0.
