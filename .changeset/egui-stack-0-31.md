---
egui-pop-core: major
egui-pop-api: major
egui-pop-viewport: major
egui-pop-agent: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
egui-pop: major
---

egui_stack 0.31. A right click on the empty part of a docked tab bar opens
the group's menu ("Move group"), as egui_stack does by default.
`AppBuilder::dock_controls` sets the docks' controls beyond their tabs
(`workbench::DockControls`): an "Eject group" button on the docked tab
bars, which moves a whole tab group into a window of its own (the
`workbench.detach` step with `group`), off by default; the group menus,
on by default; and where else than its move handle a drag moves a
floating window (`MoveFrom`, its bar by default).
`workbench::set_dock_controls` changes them while running. Breaking:
egui_stack's re-exported types (`FloatMove`, `GroupHandles`, the
`sections` types, and now `MoveFrom`) are 0.31's.
