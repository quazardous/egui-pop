---
egui-pop-api: minor
egui-pop: minor
---

The workbench gives its dock space the application's bus (egui_stack's `bus` feature): the panels' rules refuse a dragged tab through egui_stack's `stack.dock.drop` (the listener `workbench.dock_drop`), so any listener of that point may refuse a drop with its reason, asked when the destination changes and at release; the shell follows the zones resized, folded and unfolded and the flyouts from `stack.layout.changed` (`workbench.stack_layout`).
