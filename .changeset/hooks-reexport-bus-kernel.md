---
egui-pop-hooks: patch
---

The hook kernel moved to its own crate, `bus-kernel` (0.1, with a conformance suite any bus-contract implementation can run). `egui-pop-hooks` re-exports it: `egui_pop_hooks::Kernel` and every other path name the same items, and `egui_pop_hooks::bus_kernel` is new.
