---
egui-pop-core: minor
egui-pop-api: patch
egui-pop-tokens: patch
---

The guide shown while a side area's line is dragged far enough to fold
it takes `side-area.fold.border-width` and `border-color` (width 0: no
guide; a term left out keeps egui_stack's guide stroke). The edge of a
side area lying over the centre draws the hover and drag line from
`side-area.hover.border-*` and `side-area.active.border-*`, none without
them. New `Kit::fold_guide_stroke`, `paint_snap_guide`,
`paint_over_edge`, `AreaLineKit::fold`. Needs egui_stack 0.25.11.
