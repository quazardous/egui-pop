---
egui-pop-core: minor
egui-pop-api: patch
egui-pop: minor
---

The shell's side and bottom areas are laid out by egui_stack's layout
layer (egui_stack 0.25.4) instead of egui panels: each area is a zone
with its `AppBuilder` sizes, the centre grows and keeps its least width,
a folded side area is a strip whose grip unfolds it. The lines between
areas take the pointer, keys and assistive technology; a zone holding no
dock refuses a dragged tab. Sizes, old sessions' included, are kept. The
kit paints the layout: `impl LayoutPainter for Kit`,
`Kit::paint_area_strip`, `Kit::paint_area_line`.
