---
egui-pop-core: major
egui-pop-api: major
egui-pop: major
---

egui_stack 0.29. The workbench behaves as before: its side areas fold only when the user folds them (no zone folds by itself short of room), so a click on a strip still unfolds the area, or folds back one lying over the centre. Dock tabs follow the accessible tab pattern: a workbench tab is a tab (it was a button), its list is named by its dock's title, and the active tab's body is a tab panel named by its tab, so a test finding a tab by its label alone may now find two nodes: look it up with `Role::Tab`. A float over the main window with no title bar of its own has a Close window button beside its move handle. A detached window moved by the system is no longer asked back to its place (egui_stack moves a float whose place changed from code; egui-pop now leaves an open float's place to egui_stack). Breaking: egui_stack's re-exported types (`FloatMove`, `GroupHandles`, the `sections` types) are 0.29's, and the tabs' accessible role is `Tab`.
