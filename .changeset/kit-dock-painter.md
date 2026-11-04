---
egui-pop-core: minor
egui-pop-api: patch
---

The kit paints the workbench's docks through egui_stack's `DockPainter`
(`Kit` implements it): the tabs, as wide as the kit measures them (no
longer at least their label plus 16 points), the tab bars, the
separators between docked panels (the `splitter` component, no longer
egui's colours) and the drop overlay inside an area (`dock-preview`, its
buttons `button`). The tabs are painted once, where egui_stack lays them
out, instead of over its invisible ones.
