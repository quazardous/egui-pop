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

A panel can open several times at once. A panel declared with several
(`PanelSpec::instances(Instances::Multiple)`, or `Ctx::instance_panel`)
opens as many instances as asked: the panel declared is instance 1, each
new one `<kind>#<n>` (`app.gallery#2`), still a `PanelId`
(`PanelId::kind`, `PanelId::instance_number`), drawn by the kind's one
draw function, told which `Instance` it draws. Each instance holds a
payload the application owns (`AppHandle::set_instance_payload`,
`AppHandle::instance_payload`), kept in the layout (`Layout::instances`),
saved with it and handed back on restore; layouts saved before load as
ones with none. `pop.panel.open` takes `instance`: `"new"` opens a new
instance at the place asked, an instance's id opens that one; the
`workbench.panel.open` command passes it on and returns the new id. A tab
group's Open panel ▸ and "+" list "New <panel>", and so does the View
menu (`panel_menu::LIST_NEW_INSTANCES`, for an application's Window
menu). Closing an instance drops it with its payload; the journal says
`panel.instance.opened` and `panel.instance.closed`. Breaking:
`PanelSpec` and `Layout` have a new public field each (`instances`), and
`workbench::Change` a new variant, `NewInstance`.
