---
egui-pop-api: minor
egui-pop-core: patch
egui-pop-model: patch
egui-pop-i18n: patch
---

A floating panel has the panel menu a docked tab has. Its title (and each tab of a floating group) opens it on a right click, Shift+F10 or the `Menu` key, as a context menu on the bus (`menus::context_menu`, a `popup.open`): Detach into a window, Move to window ▸, Move group to window ▸, Move to group ▸ (the docked tab groups, by their panels' titles), Dock back (where it floated from: its area, among the same tabs) and Close. One declared menu serves docked, floating and detached panels (`workbench::panel_menu::MENU_PANEL`), its entries made by list providers as it opens, each a workbench command: `workbench.panel.move`, `workbench.panel.float`, `workbench.panel.detach` (`group` for the tab group), `workbench.panel.move_to_group`, `workbench.panel.dock_back`, `workbench.panel.close`; `workbench.move_to_window` takes `group`. New steps `workbench.join_group` and `workbench.dock_back` (`Change::JoinGroup`, `Change::DockBack`). `egui-pop-model`: `Layout::docked_place`, `Layout::dock_at`, `Layout::join_tabs`. `egui-pop-core`: `band::Band::response`. The strings `egui-pop-workbench-move-to-window` and `egui-pop-workbench-move-group-to-window` became the submenus' labels; "Dock back" is in English and French.
