---
egui-pop-model: major
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

A closed panel opens again where it was. `Layout::hide` keeps the place it
had (`Layout::closed`, read by `Layout::last_place`; `Layout::place_of`
gives any panel's): among the same tabs at its index, floating at its
corner, or in its window. Opening it puts it back there; that place gone,
it joins the focused panel's tab group when its areas allow it, else its
declared area. The `<panel>.toggle` command now does what it said ("opens
where it was").

Opening a panel is a point on the bus, `pop.panel.open`
(`bus::PanelOpen`, a `PanelOpenAsk` with the panel, `place`: `last`, `here`
or an area, and `group` for `here`; `AppHandle::open_panel`), answered by
the workbench through the `workbench.show` step, which takes the same
`place` and `group`. The `workbench.panel.open` command asks it for agents
and menus. A tab group's menu (a right click on its bar's empty part) has
Open panel ▸, every panel its place allows; `DockControls::add_buttons`
shows a "+" on each tab bar, off by default, listing the closed panels
that may open there. Breaking: `Layout` has a new public field, `closed`,
and `workbench::Change` a new variant, `Open`.
