---
egui-pop-hooks: major
egui-pop-api: major
egui-pop: major
egui-pop-agent: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
---

egui-pop moves to bus-contract 0.3. The hook kernel (`egui_pop_hooks::Kernel`) honours refusals on request points: a listener returning `HookError::refuse(reason)` stops the request, no later listener runs, and the result names it (`Altered::refused`, `Collected::refused`, `PendingRun::refused`); on a point declared `fail_closed` (`PointSpec::fail_closed`, `PointDecl::fail_closed`) a listener's error, panic or missed deadline refuses the request the same way, and `PointInfo::fail_closed` shows it. The kernel offers the `Defer` capability (`Bus::defer()`): `Kernel::ask::<P>` queues a request for the calling thread, `Kernel::drain` runs the thread's asks in ask order and reports each (`PendingRun::ask`), `Kernel::waiting` counts them. Breaking: bus-contract 0.3 makes its result and introspection types `#[non_exhaustive]` (built with `PendingRun::new`, `ListenerInfo::new`, `PointInfo::new`, `Failure::new`, `Altered::new`, `Collected::new`) and `HookError` an enum (`HookError::failed`, `HookError::refuse`, `message()`); a `match` on `FailureKind` or `Refused` needs a `_` arm. A custom `Bus` handed to `AppBuilder::bus` needs no new method.
