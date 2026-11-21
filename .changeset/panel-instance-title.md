---
egui-pop-api: minor
egui-pop: minor
---

The application names a panel instance's tab title: `pop.panel.retitle`
(`bus::PanelRetitle`, `bus::PanelRetitleAsk { panel, title }`), or
`AppHandle::set_instance_title(&id, Some(title))` from code; `None`
(`title: null`) gives the generated title ("Gallery 2") back. The title
is kept in the instance's layout entry, next to its payload, so it is
saved and restored with the layout (layouts without it load unchanged).
The tab, the panel menu's list of instances, the View menu,
accessibility and `Instance::title` read it, and the journal says
`panel.instance.retitled` (`{ panel, kind, title }`). Only an open
instance `<kind>#<n>` of a panel declared with several takes a title;
anything else is refused with the reason.
