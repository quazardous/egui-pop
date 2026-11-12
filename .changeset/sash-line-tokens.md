---
egui-pop-core: minor
egui-pop-tokens: patch
---

The line between a side area and the centre takes its hover and drag
looks from the kit: `side-area.hover.border-width` and `border-color`,
and their `active` twins, a width of 0 drawing no line. A theme giving
none keeps egui's widget strokes, as before. `Kit::area_line_stroke`
tells the stroke for a sash state, `Kit::area_line` holds the compiled
tokens. The side area's grip (`area-grip`) documents its `hover` and
`active` fills, so a kit can keep the grip as the only cue.
