---
egui-pop-api: minor
egui-pop-i18n: minor
egui-pop: minor
---

The tab menu's "Move group" is on by default (`DEFAULT_GROUP_HANDLES` is every handle), with its setting, `workbench.group_handle.menu`, to turn it off: its list names the areas by their translated titles and the panels they hold ("Start side bar (Tree)") rather than by their internal names, through egui_stack 0.28.6's `TabViewer::dock_title`. The French catalogue translates the new texts.
