---
egui-pop-core: major
egui-pop-api: major
egui-pop: major
---

egui_stack 0.28.2: each tab bar ends with a grip that moves its whole group, and the user chooses the group gestures in the settings: the key held to drag a tab's group (`workbench.group_drag_modifier`: shift, ctrl or alt), each group handle (`workbench.group_handle.*`), and what moving a floating window does (`workbench.float_move`: move or dock). `AppBuilder::group_handles` sets the handles an application allows (all but the tab menu's by default). Breaking: egui_stack's re-exported types are 0.28's.
