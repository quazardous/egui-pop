---
egui-pop-api: minor
egui-pop-core: minor
---

A shortcut in progress belongs to one window. Each detached panel's window keys with its own dispatcher, as the main window and each application window do, so the first key of a chord pressed in one window and its second in another runs nothing. Another window becoming the active one (`window.activate`) drops every window's shortcut in progress: the bus's local listener `bus::KEYS` (`window.keys`, after the registry) says so. The keymap and the `appWindow` / `window` when-context are unchanged. `egui-pop-core`: `KeyDispatcher::cancel_pending` drops a shortcut in progress.
