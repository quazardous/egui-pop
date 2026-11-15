---
egui-pop-model: minor
egui-pop-api: minor
egui-pop-i18n: patch
---

What a plugin leaves on screen when it goes is closed, greyed or kept for its return (ADR 0028). A dialog open on one of its commands, or a custom dialog it opened, is cancelled at once: `dialog.cancelled` from `egui_pop` with a `reason` ("its plugin `x` is disconnected"), and the next queued dialog shows. A toast's action naming one of its commands is greyed, its tooltip naming the plugin (`AppHandle::command_gone_with`), and alive again when it reconnects. Its panels' places are kept in the layout, dormant and saved (`SavedLayout::dormant`, `AppHandle::saved_layout`, `Layout::repair_dormant`: docked beside the same tabs, detached, floating or closed), and a panel declared again goes back there instead of to its default area. Its components, host offers and their mounts, and the mounts made while its code ran, are removed; `plugin.disconnected` counts them (`components`, `hosts`, `mounts`).
