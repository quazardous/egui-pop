---
egui-pop-core: patch
egui-pop-api: patch
egui-pop: patch
---

`kit::State::with(flag, false)` now clears the flag, as its callers read
it; it used to leave a flag already set, so a selected places item under
the pointer kept `hover` and a theme's hover fill covered its selected
fill. `State::without(flag)` clears a flag. The places panel's selected
item keeps its selected fill under the pointer, the focus outline over
it; a window button hovered by egui and a dock grip pressed keep their
look.
