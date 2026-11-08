---
egui-pop-core: minor
egui-pop-api: patch
egui-pop: minor
---

The shell's side and bottom areas are laid out by egui_stack's layout
layer (egui_stack 0.25.4) instead of egui side panels: each area is a
zone with its least, largest and default sizes from `AppBuilder`, the
centre the zone that grows and keeps its least width, a folded side area
a strip of the layout whose grip unfolds it. The lines between the areas
answer the pointer, the arrows (Shift for bigger steps), Enter and a
double click (back to the default size), and assistive technology. A
zone holding no dock refuses a dragged tab (`DockSpace::bind`). The
sizes the user gave are kept as before, and a session saved with the
former panels keeps its areas' sizes. The kit paints the layout:
`impl LayoutPainter for Kit` and `Kit::paint_area_strip`.
