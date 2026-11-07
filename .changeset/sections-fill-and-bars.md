---
egui-pop-core: minor
egui-pop: minor
---

`Sections::fill(false)` sizes a stack of sections by its content: an
open section sized by the list is never taller than what its body lays
out, the long ones share what is left, the folded bands follow right
under the last open section and the room left stays empty under the
last band. A section's scroll bar is now drawn as `UiPop::scroll_area`
draws one: the theme's thumb colours and track (it kept the default
kit's colours), and `scroll-bar.margin-track`. Needs egui_stack 0.25.2.
