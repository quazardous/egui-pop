---
egui-pop-api: major
egui-pop: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
---

The workbench's areas share one egui_stack dock space (egui_stack 0.25.1): a tab dragged from one area to another gets the drop zones it gets inside one.

- **Any zone between areas.** Over another area, the middle of a group joins its tabs, an edge splits the group on that side, a tab bar takes the tab between two tabs, with the same preview as inside an area. Before, a tab dropped on another area could only join the end of its first tabs.
- **Refusals with their reason.** An area a panel stays out of shows the zone disabled, with the reason ("Log stays out of the start side bar"); a drop there changes nothing.
- **Closed and bare areas.** A closed side or bottom area is still offered along the areas' edge, and an area drawn bare (a zone alone) over its whole rect: a tab dropped there joins its tabs.
- **Still a step.** A tab dropped in another area is moved through `workbench.move`, run after the drop: a plugin's veto puts it back where it was, a redirect sends it where the step says. `workbench.move` for a panel already in the area now keeps it where it is.
- **The drop hint** (`AppHandle::drop_hint`, the debug bridge's `workbench` collector) says what the dock resolved: `DropHint::zone` (an egui_stack `DropZone`: `Center` joins tabs, an edge splits) and `index` among the tabs it would join, its rect where the zone is drawn; `preview`, the ghost tab, is now a palette's only. `DropHint` is `#[non_exhaustive]`.
- **Saved layouts** keep their shape: each area's tree becomes its dock, so a layout saved before reads as it did.
