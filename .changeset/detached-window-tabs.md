---
egui-pop-model: major
egui-pop-core: major
egui-pop-api: major
egui-pop: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-viewport: major
egui-pop-views: major
---

A detached panel's window takes other panels as tabs, dropped on it or moved there from the tab menu.

- **Drops on a detached window.** While a tab is dragged, a detached window under the pointer shows the drop zone the areas show (the kit's `dock-preview`, picked with egui_stack's `DropZone::at`), disabled with its reason for a panel that may not leave the main window; let go there, the tab joins the window's tabs. The window sees the pointer through the main one: always when it is embedded in it, when both windows know their place on the screen otherwise (not on Wayland).
- **Move to window.** The tab menu lists the detached windows ("Move to window", in English and French), which works everywhere; an agent runs the new `workbench.move_to_window` command (`panel`, the focused one by default; `window`, a panel of that window), and code `AppHandle::move_to_window`. Each way is one new `workbench.join_window` step. `AppHandle::window_drop` (and the debug snapshot's `window_drop`) says which window a drag is over.
- **Tabs in a window.** The window shows one tab bar (no splits) and is titled after the tab shown; closing it docks every panel it holds, Attach one of them docks that one, Detach gives it a window of its own.
- **The layout document** keeps a window's first panel under `panel` and the others under `tabs`, the shown one as `shown` (`layout::Auxiliary::tabs`, `shown`, `Auxiliary::window`, `Layout::window_of`, `Layout::join_window`). Layouts saved before read as they did; an application that predates this opens the first panel in the window and docks the others.

Migration: build an `egui_pop::model::layout::Auxiliary` with `Auxiliary::window(panel, screen, rect)` rather than a struct literal. `workbench::Change` has a new variant, `Change::JoinWindow`.
