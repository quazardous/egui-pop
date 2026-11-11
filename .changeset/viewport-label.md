---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop-viewport: minor
egui-pop: minor
---

Labels over images, and the axes' colours. A new kit component,
`viewport-label`, draws a label over an image: a pill whose fill is
translucent so the image shows through, with its text colour, radius
and padding from the theme. An overlay's label (`Overlay::label`), until
now bare text in the line's colour, is drawn through it, and
`egui_pop::viewport::overlay::paint_label` draws one anywhere, for a
HUD's readout on an image. Its text is checked to read on its fill over
black and over white alike (`contrast::OVER_IMAGES`, part of
`contrast::check`). New system tokens `palette.axis.x`, `.y` and `.z`
(red, green and blue, `Tokens::of(ctx).palette.axis`) colour a view's
axes; every built-in theme gives them, seen at 3:1 on the viewport's
background. The Rerun-like kit gives both Rerun's values.
