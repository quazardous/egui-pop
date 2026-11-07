---
egui-pop-api: minor
egui-pop: minor
---

`AppBuilder::bare_strips(&[Area])` keeps a side area's folded strip to
its grip glyph: no icon (or title's first letter) per panel, the strip
`area-strip.width` wide, a click unfolding the area as before.
`AppHandle::bare_strip(area)` says which areas are bare, and
`workbench::strip` lists no icon for them.
