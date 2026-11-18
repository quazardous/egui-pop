---
egui-pop-api: minor
egui-pop: minor
---

A local listener can be registered for a window: `AppHandle::listen_local_in::<P>(window, …)` wakes that window each time it runs, so a fact heard for a detached window draws it within one frame, with no hand-written `request_repaint_of`. The main window is still woken to run what other threads queued and what was asked for after the drawing. A window drawn in the main one (embedded, a detached panel's) or not open wakes the main window. `AppHandle::wake_window` asks a window's pass the same way.
