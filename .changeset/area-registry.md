---
egui-pop-core: minor
egui-pop-viewport: minor
egui-pop-api: minor
egui-pop: minor
---

The area registry (`egui_pop_core::areas`): an interactive area registers each pass with what it writes (display, navigate or edit) and, for an edit area, the control it writes, its value and path. The framework draws what tells them apart: an edit area's grips at rest, the cursor of its drag and a hover card naming the control and its value (also its accessible description); display areas get none. An edit area is inert while its panel's state is not current (`pop.panel.state` stale, updating or failed): no grip, no card, no edit. The histogram registers as an edit area when draggable, its exposure grip on its right edge (`Histogram::edits` names what the drag writes), and as a display area otherwise. `pop.ui.areas` (`bus::UiAreas`, `AppHandle::ui_areas`) lists each window's areas on the bus. The gallery's images page shows a histogram with its grip beside a scope without one.
