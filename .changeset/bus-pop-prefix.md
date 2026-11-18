---
egui-pop-api: major
egui-pop: major
egui-pop-views: major
---

Every bus point egui-pop declares and every listener it registers takes the `pop.` prefix (bus-contract's "Naming points": `<crate>.<subject>.<moment>`), so several libraries can share one bus: `window.close` is now `pop.window.close`, `command.run` is `pop.command.run`, `layout.changed` is `pop.layout.changed`, the listener `window.registry` is `pop.window.registry`, and so on for all 24 points (`pop.ui.areas` included) and 33 listeners. Code using the point types (`bus::WindowClose`…) and the listener constants (`bus::REGISTRY`…) keeps working; a listener ordered against an egui-pop listener by its id (`after("window.registry")`) or matching a point by its id string must take the new name. Command ids (`job.cancel`, `window.close`), step ids (`window.close`, `layout.save`), journal event names and settings keys do not change. A test fails on an egui-pop point or listener without the prefix.
