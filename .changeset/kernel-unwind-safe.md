---
egui-pop-hooks: patch
---

`Kernel` is `UnwindSafe` and `RefUnwindSafe` again with a host journal and relay: they are held as unwind safe, as the rest of its shared state is.
