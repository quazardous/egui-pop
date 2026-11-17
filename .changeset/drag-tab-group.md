---
egui-pop-api: major
egui-pop-model: minor
---

A whole tab group can be dragged by the empty part of its tab bar, as an editor group is dragged by its tab bar in established IDEs: all its tabs move together, in order, the shown one kept, into another group or beside it (each tab's `workbench.move`), onto a detached window (its tabs joined whole, with the same frames and outlines as a tab, positions hidden or not), or outside every window, where it opens a window of its own. The tab's menu offers "Move group to window", naming each detached window and a new one, for keyboard users and Wayland. Across windows they are `workbench.join_window` and `workbench.detach` with `group: true`, the new `Change::JoinWindowGroup` and `Change::DetachGroup` (a new variant of the public `Change` enum, hence the major bump); a group goes whole or not at all. `egui-pop-model`: `Layout::group_of`, `Layout::join_window_group` and `Layout::detach_group`. egui_stack is taken at its tab-group drag (`DockArea::dragged_node`).

**Breaking:** to move, a `match` on `workbench::Change` adds arms for `JoinWindowGroup` and `DetachGroup`, or a wildcard arm.
