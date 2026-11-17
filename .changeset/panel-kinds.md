---
egui-pop-model: major
egui-pop-api: major
egui-pop: minor
---

A panel declares what it does to what the application edits: `PanelSpec::kind` takes a `panels::PanelKind` (`View`, the default; `Control`, its purpose is editing; `Mixed`, a view with parts that edit), a new field of `PanelSpec`. Behaviour change: the chrome now shows an "edits" mark ("Edits your work", "Parts of it edit your work") on the tab of a control or mixed panel and after its floating palette's title, never for a view panel; it takes the tab's one mark slot (`bus::TabMark::Edits`, the lowest rank), so a busy or failed state shows instead while it lasts, and a floating palette's band now shows those state marks too. `panel.describe` on the bus (`bus::PanelDescribe`, `bus::PanelDescription`, listener `bus::PANEL_DESCRIBE`, listed in `bus::PANEL_DESCRIBE_POINTS`, `bus::POINTS` and `bus::CONTRACT`; `AppHandle::describe_panel`) describes a declared panel: its title, whether it is a palette, and its kind.
