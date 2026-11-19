---
egui-pop-core: major
egui-pop-api: major
egui-pop: major
---

egui_stack 0.29. The workbench behaves as before: its side areas fold only when the user folds them (no zone folds by itself short of room), so a click on a strip still unfolds the area, or folds back one lying over the centre, and it takes no drop on a folded strip. Dock tabs follow the accessible tab pattern: the active tab's body is a tab panel named by its tab, and each tab list is named by its dock's title. A float over the main window with no title bar of its own has a Close window button beside its move handle. Breaking: egui_stack's re-exported types (`FloatMove`, `GroupHandles`, the `sections` types) are 0.29's.
