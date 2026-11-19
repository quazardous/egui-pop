---
egui-pop-hooks: major
egui-pop: major
egui-pop-agent: major
egui-pop-api: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
---

The hook kernel moved to its own crate, `bus-kernel` (0.1, with a conformance suite any bus-contract implementation can run). `egui-pop-hooks` re-exports it: `egui_pop_hooks::Kernel` and every other path name the same items, and `egui_pop_hooks::bus_kernel` is new.

Migration: `egui_pop_hooks` now re-exports `bus-kernel`, and its types are unchanged; code using `egui_pop_hooks::Kernel` and the other paths builds as before.
