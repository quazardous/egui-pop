---
egui-pop-api: major
egui-pop: major
---

The detached panels' windows are egui_stack 0.28.4's native floats: their tabs are docks of the workbench's space, painted by the kit, with egui_stack's move handle and the Dock back button at the end of the tab bar, and egui-pop's viewport ids. A tab or a group dragged between windows is one drag of the space (`stack.dock.drop` vetoes it in any window; on Wayland every window outlines its docks), and a drop into or out of a detached window still runs `workbench.join_window` or `workbench.move`.
