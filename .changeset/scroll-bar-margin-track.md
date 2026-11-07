---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop: minor
---

`scroll-bar.margin-track` keeps a scroll bar's thumb that many points
from both ends of its track (0 by default, as before), in every area
shown through `UiPop::scroll_area` and its variants: egui's
`ScrollStyle` has no such field, so the area is given the bar's
rectangle (`ScrollBarKit::margin_track`, `ScrollBarKit::track_rect`).
