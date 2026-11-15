---
egui-pop-hooks: minor
---

A new crate, `egui-pop-hooks`: a hook kernel without egui or dependencies, for extension points where several modules contribute in order to one result. Hook points are types (an id, the input listeners read, the value they edit); listeners have ids, `after`/`before` constraints and a priority (a cycle is refused at `listen`, a missing target reported and ignored). `Kernel::alter` passes a value through the listeners in order, `Kernel::collect` gathers each listener's own value. A listener's edit counts only when it returns `Ok` without panicking (panics are caught) within its deadline (1 s by default, overridable per point and per listener; threaded listeners are abandoned at it); every failure is returned and sent to the `on_failure` sinks, so a host can publish a `hook.failed` fact. `Kernel::points` and `Kernel::listeners` list every point and its listeners in run order with their counters.
