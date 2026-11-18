---
egui-pop-hooks: minor
egui-pop: minor
---

The hook kernel offers bus-contract 0.3.1's `Timings` capability: every listener run is timed into a log histogram of relaxed atomics, per listener and per pass (count, total, max, p50, p95; `Kernel::timings`, `point_timings`, `reset_timings`, `set_timing`), and a pass over its point's budget (`PointDecl::budget`, `PointSpec::budget`) goes to the `on_over_budget` sinks with each listener's time, its results kept.
