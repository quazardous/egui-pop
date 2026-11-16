---
egui-pop-hooks: major
egui-pop-api: major
---

The bus is a contract the application supplies (ADR 0030). The contract lives in its own crate, `bus-contract`: hook points with a declared kind (`PointKind`: request, event, alter, collect), the object-safe `Bus` trait (declare, listen with owner and `after` / `before` / `last`, unlisten, forget, alter, collect, failure sinks, introspection), the typed calls on `dyn Bus`, a const contract table (`PointDecl`, `declare_all`), `order_dump` and `missing_targets`. `egui-pop-hooks`'s `Kernel` implements it and re-exports its types under their old paths; `PointInfo` gains `kind`. `AppBuilder::bus` hands egui-pop the application's bus (without it, a kernel of its own, as before); egui-pop declares its points from `bus::CONTRACT` and registers its listeners on it. `AppHandle::bus` returns an `Arc<dyn bus_contract::Bus>`; `bus::order_dump` and `bus::missing_targets` take `&dyn Bus` (re-exported from the contract); a second application on one bus is refused with `BuildError::Bus`. Code calling `bus()` and the typed methods compiles unchanged.
