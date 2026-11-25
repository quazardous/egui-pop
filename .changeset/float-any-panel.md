---
egui-pop-model: minor
egui-pop-api: major
egui-pop-i18n: major
egui-pop-agent: major
egui-pop-editors: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
egui-pop: major
---

Every panel may float unless its spec opts out (`PanelSpec::floatable(false)`):
`PanelSpec::new` now makes a floatable panel. The panel menu always lists
Float for a docked panel, greyed with its reason when the panel may not
float. "Float group" (the tab menu and the group menu) floats a docked tab
group whole, as one floating group of tabs (`Change::FloatGroup`,
`workbench.panel.float` with `group`, `Layout::float_tabs`); Dock back puts
the whole group back where it was. The new setting "Floating panels"
(`workbench.float_panels`: `any` or `palettes`) narrows which panels may
float.

To move: `Change::FloatGroup` is a new variant of `workbench::Change`;
an exhaustive `match` on it takes an arm for it.
